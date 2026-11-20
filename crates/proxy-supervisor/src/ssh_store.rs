//! Persistent store for SSH credentials, hosts, and standalone local port forwards.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use proxy_config::{atomic_write, optional_json, security::protect_directory};
use proxy_model::{
    DEFAULT_SSH_PORT, Error, Result, SshAuthKind, SshCredentialInput, SshCredentialSummary,
    SshForward, SshHost, id, resolve_path,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct StoredCredential {
    id: String,
    name: String,
    username: String,
    #[serde(default)]
    auth_kind: SshAuthKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    encrypted_password: Option<String>,
    #[serde(default)]
    private_key_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    encrypted_passphrase: Option<String>,
}

impl StoredCredential {
    fn summary(&self) -> SshCredentialSummary {
        SshCredentialSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            username: self.username.clone(),
            auth_kind: self.auth_kind,
            has_password: self
                .encrypted_password
                .as_deref()
                .is_some_and(|v| !v.is_empty()),
            private_key_path: self.private_key_path.clone(),
            has_passphrase: self
                .encrypted_passphrase
                .as_deref()
                .is_some_and(|v| !v.is_empty()),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SshPersistedFile {
    #[serde(default)]
    credentials: Vec<StoredCredential>,
    #[serde(default)]
    hosts: Vec<SshHost>,
    #[serde(default)]
    forwards: Vec<SshForward>,
}

/// Decrypted runtime credential passed internally to the SSH client without logging or returning over IPC.
#[derive(Clone)]
pub struct ResolvedCredential {
    pub username: String,
    pub auth_kind: SshAuthKind,
    pub password: Option<String>,
    pub private_key_path: PathBuf,
    pub passphrase: Option<String>,
}

pub struct SshStore {
    root: PathBuf,
    path: PathBuf,
    data: SshPersistedFile,
}

impl SshStore {
    pub fn load(root: &Path) -> Result<Self> {
        let dir = root.join("ssh");
        let path = dir.join("config.json");
        let data = optional_json::<SshPersistedFile>(&path)?.unwrap_or_default();
        Ok(Self {
            root: root.to_path_buf(),
            path,
            data,
        })
    }

    pub fn summaries(&self) -> Vec<SshCredentialSummary> {
        self.data
            .credentials
            .iter()
            .map(StoredCredential::summary)
            .collect()
    }

    pub fn hosts(&self) -> &[SshHost] {
        &self.data.hosts
    }

    pub fn forwards(&self) -> &[SshForward] {
        &self.data.forwards
    }

    pub fn save_credential(&mut self, input: SshCredentialInput) -> Result<SshCredentialSummary> {
        let name = input.name.trim().to_owned();
        if name.is_empty() {
            return Err(Error::new("SSH_CREDENTIAL_INVALID", "凭证名称不能为空"));
        }
        let username = input.username.trim().to_owned();
        if username.is_empty() || username.chars().any(char::is_control) {
            return Err(Error::new("SSH_CREDENTIAL_INVALID", "SSH 账号不能为空"));
        }

        let existing = if input.id.trim().is_empty() {
            None
        } else {
            self.data
                .credentials
                .iter()
                .find(|c| c.id == input.id.trim())
                .cloned()
        };

        let (encrypted_password, private_key_path, encrypted_passphrase) = match input.auth_kind {
            SshAuthKind::Password => {
                let encrypted_password = match input.password.as_deref() {
                    Some(pw) if !pw.is_empty() => Some(protect_secret(pw.as_bytes())?),
                    _ => Some(
                        existing
                            .as_ref()
                            .and_then(|c| c.encrypted_password.clone())
                            .ok_or_else(|| Error::new("SSH_CREDENTIAL_INVALID", "密码不能为空"))?,
                    ),
                };
                (encrypted_password, PathBuf::new(), None)
            }
            SshAuthKind::PrivateKey => {
                if input.private_key_path.as_os_str().is_empty() {
                    return Err(Error::new(
                        "SSH_CREDENTIAL_INVALID",
                        "请选择或填写私钥文件路径",
                    ));
                }
                let resolved_key = resolve_path(&self.root, &input.private_key_path);
                let key_text = fs::read_to_string(&resolved_key).map_err(|e| {
                    Error::new(
                        "SSH_KEY_UNREADABLE",
                        format!("无法读取私钥文件 {}: {e}", resolved_key.display()),
                    )
                })?;

                let encrypted_passphrase = match input.passphrase.as_deref() {
                    Some("") => None,
                    Some(phrase) => Some(protect_secret(phrase.as_bytes())?),
                    None => existing
                        .as_ref()
                        .and_then(|c| c.encrypted_passphrase.clone()),
                };
                let check_passphrase = encrypted_passphrase
                    .as_deref()
                    .map(unprotect_secret_string)
                    .transpose()?;
                russh::keys::decode_secret_key(&key_text, check_passphrase.as_deref()).map_err(
                    |e| Error::new("SSH_KEY_INVALID", format!("私钥解析或口令校验失败：{e}")),
                )?;
                (
                    existing.as_ref().and_then(|c| c.encrypted_password.clone()),
                    input.private_key_path.clone(),
                    encrypted_passphrase,
                )
            }
        };

        let id_val = if input.id.trim().is_empty() {
            id()
        } else {
            input.id.trim().to_owned()
        };
        let stored = StoredCredential {
            id: id_val.clone(),
            name,
            username,
            auth_kind: input.auth_kind,
            encrypted_password,
            private_key_path,
            encrypted_passphrase,
        };
        let summary = stored.summary();
        if let Some(slot) = self.data.credentials.iter_mut().find(|c| c.id == id_val) {
            *slot = stored;
        } else {
            self.data.credentials.push(stored);
        }
        self.persist()?;
        Ok(summary)
    }

    pub fn delete_credential(&mut self, id: &str) -> Result<()> {
        let id = id.trim();
        if let Some(host) = self.data.hosts.iter().find(|h| h.credential_id == id) {
            return Err(Error::new(
                "SSH_CREDENTIAL_IN_USE",
                format!(
                    "凭证正被跳板机「{}」引用，请先修改或删除该跳板机",
                    host.name
                ),
            ));
        }
        let before = self.data.credentials.len();
        self.data.credentials.retain(|c| c.id != id);
        if self.data.credentials.len() == before {
            return Err(Error::new("SSH_NOT_FOUND", "未找到指定的 SSH 凭证"));
        }
        self.persist()
    }

    pub fn save_host(&mut self, mut host: SshHost) -> Result<SshHost> {
        host.name = host.name.trim().to_owned();
        if host.name.is_empty() {
            return Err(Error::new("SSH_HOST_INVALID", "跳板机名称不能为空"));
        }
        host.host = host.host.trim().to_owned();
        if host.host.is_empty()
            || host.host.len() > 255
            || host
                .host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(Error::new(
                "SSH_HOST_INVALID",
                "跳板机地址不能为空或包含空白",
            ));
        }
        if host.port == 0 {
            host.port = DEFAULT_SSH_PORT;
        }
        if !self
            .data
            .credentials
            .iter()
            .any(|c| c.id == host.credential_id)
        {
            return Err(Error::new("SSH_HOST_INVALID", "请选择有效的 SSH 凭证"));
        }
        host.host_key_fingerprint = host.host_key_fingerprint.trim().to_owned();
        if host.id.trim().is_empty() {
            host.id = id();
        } else {
            host.id = host.id.trim().to_owned();
        }
        if let Some(slot) = self.data.hosts.iter_mut().find(|h| h.id == host.id) {
            *slot = host.clone();
        } else {
            self.data.hosts.push(host.clone());
        }
        self.persist()?;
        Ok(host)
    }

    pub fn delete_host(&mut self, id: &str) -> Result<()> {
        let id = id.trim();
        if let Some(fwd) = self.data.forwards.iter().find(|f| f.ssh_host_id == id) {
            return Err(Error::new(
                "SSH_HOST_IN_USE",
                format!(
                    "跳板机正被端口转发「{}」引用，请先删除或修改该转发",
                    fwd.name
                ),
            ));
        }
        let before = self.data.hosts.len();
        self.data.hosts.retain(|h| h.id != id);
        if self.data.hosts.len() == before {
            return Err(Error::new("SSH_NOT_FOUND", "未找到指定的 SSH 跳板机"));
        }
        self.persist()
    }

    pub fn save_forward(&mut self, mut forward: SshForward) -> Result<SshForward> {
        forward.name = forward.name.trim().to_owned();
        if forward.name.is_empty() {
            return Err(Error::new("SSH_FORWARD_INVALID", "转发名称不能为空"));
        }
        forward.listen = forward.listen.trim().to_owned();
        if forward.listen.parse::<IpAddr>().is_err() {
            return Err(Error::new(
                "SSH_FORWARD_INVALID",
                "本地监听地址必须为有效 IP",
            ));
        }
        if forward.port == 0 {
            return Err(Error::new(
                "SSH_FORWARD_INVALID",
                "本地端口必须在 1–65535 之间",
            ));
        }
        forward.target_host = forward.target_host.trim().to_owned();
        if forward.target_host.is_empty()
            || forward
                .target_host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(Error::new(
                "SSH_FORWARD_INVALID",
                "目标地址不能为空或包含空白",
            ));
        }
        if forward.target_port == 0 {
            return Err(Error::new(
                "SSH_FORWARD_INVALID",
                "目标端口必须在 1–65535 之间",
            ));
        }
        if !self.data.hosts.iter().any(|h| h.id == forward.ssh_host_id) {
            return Err(Error::new("SSH_FORWARD_INVALID", "请选择有效的 SSH 跳板机"));
        }
        let fwd_id = if forward.id.trim().is_empty() {
            id()
        } else {
            forward.id.trim().to_owned()
        };
        forward.id = fwd_id.clone();

        if let Some(other) = self.data.forwards.iter().find(|f| {
            f.id != fwd_id && f.port == forward.port && listen_overlaps(&f.listen, &forward.listen)
        }) {
            return Err(Error::new(
                "SSH_FORWARD_PORT_CONFLICT",
                format!(
                    "端口 {} 已被另一条 SSH 转发「{}」占用",
                    forward.port, other.name
                ),
            ));
        }

        if let Some(slot) = self.data.forwards.iter_mut().find(|f| f.id == fwd_id) {
            *slot = forward.clone();
        } else {
            self.data.forwards.push(forward.clone());
        }
        self.persist()?;
        Ok(forward)
    }

    pub fn delete_forward(&mut self, id: &str) -> Result<()> {
        let id = id.trim();
        let before = self.data.forwards.len();
        self.data.forwards.retain(|f| f.id != id);
        if self.data.forwards.len() == before {
            return Err(Error::new("SSH_NOT_FOUND", "未找到指定的 SSH 端口转发"));
        }
        self.persist()
    }

    pub fn resolve_for_forward(
        &self,
        forward_id: &str,
    ) -> Result<(SshForward, SshHost, ResolvedCredential)> {
        let forward = self
            .data
            .forwards
            .iter()
            .find(|f| f.id == forward_id)
            .cloned()
            .ok_or_else(|| Error::new("SSH_NOT_FOUND", "未找到指定的 SSH 端口转发"))?;
        let host = self
            .data
            .hosts
            .iter()
            .find(|h| h.id == forward.ssh_host_id)
            .cloned()
            .ok_or_else(|| Error::new("SSH_NOT_FOUND", "未找到关联的 SSH 跳板机"))?;
        let cred = self
            .data
            .credentials
            .iter()
            .find(|c| c.id == host.credential_id)
            .ok_or_else(|| Error::new("SSH_NOT_FOUND", "未找到关联的 SSH 凭证"))?;
        let password = cred
            .encrypted_password
            .as_deref()
            .map(unprotect_secret_string)
            .transpose()?;
        let passphrase = cred
            .encrypted_passphrase
            .as_deref()
            .map(unprotect_secret_string)
            .transpose()?;
        let private_key_path = if cred.private_key_path.as_os_str().is_empty() {
            PathBuf::new()
        } else {
            resolve_path(&self.root, &cred.private_key_path)
        };
        Ok((
            forward,
            host,
            ResolvedCredential {
                username: cred.username.clone(),
                auth_kind: cred.auth_kind,
                password,
                private_key_path,
                passphrase,
            },
        ))
    }

    fn persist(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
            protect_directory(dir)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.data)?;
        atomic_write(&self.path, &bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

pub fn listen_overlaps(a: &str, b: &str) -> bool {
    let Ok(ip_a) = a.trim().parse::<IpAddr>() else {
        return a.trim() == b.trim();
    };
    let Ok(ip_b) = b.trim().parse::<IpAddr>() else {
        return a.trim() == b.trim();
    };
    ip_a == ip_b || ip_a.is_unspecified() || ip_b.is_unspecified()
}

fn protect_secret(plaintext: &[u8]) -> Result<String> {
    #[cfg(windows)]
    {
        let cipher = dpapi_protect(plaintext)?;
        Ok(format!("dpapi:v1:{}", STANDARD.encode(&cipher)))
    }
    #[cfg(not(windows))]
    {
        Ok(format!("plain:v1:{}", STANDARD.encode(plaintext)))
    }
}

fn unprotect_secret_string(encoded: &str) -> Result<String> {
    let raw = if let Some(b64) = encoded.strip_prefix("dpapi:v1:") {
        let _cipher = STANDARD
            .decode(b64)
            .map_err(|e| Error::new("SSH_SECRET_CORRUPT", format!("凭证密文格式无效：{e}")))?;
        #[cfg(windows)]
        {
            dpapi_unprotect(&_cipher)?
        }
        #[cfg(not(windows))]
        {
            return Err(Error::new(
                "SSH_SECRET_UNSUPPORTED",
                "当前平台无法解密 Windows DPAPI 凭证",
            ));
        }
    } else if let Some(b64) = encoded.strip_prefix("plain:v1:") {
        STANDARD
            .decode(b64)
            .map_err(|e| Error::new("SSH_SECRET_CORRUPT", format!("凭证数据损坏：{e}")))?
    } else {
        return Err(Error::new("SSH_SECRET_CORRUPT", "未知的凭证存储格式"));
    };
    String::from_utf8(raw)
        .map_err(|e| Error::new("SSH_SECRET_CORRUPT", format!("凭证解码失败：{e}")))
}

#[cfg(windows)]
fn dpapi_protect(plaintext: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(plaintext.len())
            .map_err(|_| Error::new("SSH_SECRET_TOO_LARGE", "凭证长度超出限制"))?,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points to `plaintext` for the duration of this synchronous FFI call and
    // `output` is owned by Crypt32 until freed below with `LocalFree`.
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(Error::new(
            "SSH_DPAPI_FAILED",
            format!("DPAPI 加密凭证失败：{}", std::io::Error::last_os_error()),
        ));
    }
    let slice =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(slice)
}

#[cfg(windows)]
fn dpapi_unprotect(ciphertext: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(ciphertext.len())
            .map_err(|_| Error::new("SSH_SECRET_TOO_LARGE", "凭证密文长度超出限制"))?,
        pbData: ciphertext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points to `ciphertext` for this synchronous call; `output` is freed below.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(Error::new(
            "SSH_DPAPI_FAILED",
            format!("DPAPI 解密凭证失败：{}", std::io::Error::last_os_error()),
        ));
    }
    let slice =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(slice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn credentials_hosts_and_forwards_persist_redacted_and_enforce_references() {
        let dir = tempdir().unwrap();
        let mut store = SshStore::load(dir.path()).unwrap();
        let summary = store
            .save_credential(SshCredentialInput {
                id: String::new(),
                name: "运维账号".into(),
                username: "ops".into(),
                auth_kind: SshAuthKind::Password,
                password: Some("super-secret-password".into()),
                private_key_path: PathBuf::new(),
                passphrase: None,
            })
            .unwrap();
        assert!(summary.has_password);
        assert_eq!(summary.username, "ops");

        // Updating display name without providing a password preserves the stored secret.
        let updated = store
            .save_credential(SshCredentialInput {
                id: summary.id.clone(),
                name: "生产运维账号".into(),
                username: "ops-prod".into(),
                auth_kind: SshAuthKind::Password,
                password: None,
                private_key_path: PathBuf::new(),
                passphrase: None,
            })
            .unwrap();
        assert_eq!(updated.id, summary.id);
        assert!(updated.has_password);

        // Disk file never contains raw plaintext password string directly.
        let disk = fs::read_to_string(dir.path().join("ssh/config.json")).unwrap();
        assert!(!disk.contains("super-secret-password"));

        let host = store
            .save_host(SshHost {
                id: String::new(),
                name: "堡垒机".into(),
                host: "10.0.0.2".into(),
                port: 2222,
                credential_id: summary.id.clone(),
                host_key_fingerprint: "SHA256:abc".into(),
            })
            .unwrap();
        assert!(store.delete_credential(&summary.id).is_err());

        let fwd = store
            .save_forward(SshForward {
                id: String::new(),
                name: "生产库".into(),
                listen: "127.0.0.1".into(),
                port: 13306,
                target_host: "10.0.1.5".into(),
                target_port: 3306,
                ssh_host_id: host.id.clone(),
            })
            .unwrap();
        assert!(store.delete_host(&host.id).is_err());

        let (_, _, resolved) = store.resolve_for_forward(&fwd.id).unwrap();
        assert_eq!(resolved.username, "ops-prod");
        assert_eq!(resolved.password.as_deref(), Some("super-secret-password"));

        store.delete_forward(&fwd.id).unwrap();
        store.delete_host(&host.id).unwrap();
        store.delete_credential(&summary.id).unwrap();
    }
}
