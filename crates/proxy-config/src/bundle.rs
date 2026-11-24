//! Versioned full-configuration bundle format (`proxy-pilot-bundle`) with optional
//! `Argon2id + AES-256-GCM` passphrase encryption and secret-free SSH tunnel topology.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use proxy_model::{
    Error, PacDraft, PacMode, PausedPortForward, ProxyMode, Result, SshAuthKind, SshState,
};
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
};

pub const BUNDLE_FORMAT: &str = "proxy-pilot-bundle";
pub const BUNDLE_SCHEMA_VERSION: u32 = 1;
pub const SHARE_CODE_PREFIX: &str = "ppbundle1:";
pub const MAX_BUNDLE_BYTES: usize = 8 * 1024 * 1024;
const BUNDLE_AAD: &[u8] = b"proxy-pilot-bundle:v1:argon2id-aes256gcm";

const DEFAULT_M_COST: u32 = 19_456;
const DEFAULT_T_COST: u32 = 2;
const DEFAULT_P_COST: u32 = 1;
const MAX_M_COST: u32 = 262_144;
const MAX_T_COST: u32 = 16;
const MAX_P_COST: u32 = 8;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleSshCredentialHint {
    pub username: String,
    pub auth_kind: SshAuthKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleSshHost {
    pub ref_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub host_key_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_hint: Option<BundleSshCredentialHint>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleSshForward {
    pub name: String,
    pub listen: String,
    pub port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub host_ref: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleSshTopology {
    #[serde(default)]
    pub hosts: Vec<BundleSshHost>,
    #[serde(default)]
    pub forwards: Vec<BundleSshForward>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleUiMetadata {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub node_names: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub port_forward_names: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paused_port_forwards: Vec<PausedPortForward>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleProxyPolicy {
    pub proxy_mode: ProxyMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pac: Option<PacDraft>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleProfile {
    pub suggested_name: String,
    pub config_json: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundlePayload {
    pub profile: BundleProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_metadata: Option<BundleUiMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_policy: Option<BundleProxyPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_topology: Option<BundleSshTopology>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BundleEncryption {
    None,
    Argon2idAes256Gcm {
        m_cost: u32,
        t_cost: u32,
        p_cost: u32,
        salt: String,
        nonce: String,
        ciphertext: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleEnvelope {
    pub format: String,
    pub schema_version: u32,
    pub exported_at_utc: String,
    pub app_version: String,
    pub encryption: BundleEncryption,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<BundlePayload>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedBundleInput {
    Plain(BundleEnvelope, Box<BundlePayload>),
    Encrypted(BundleEnvelope),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BundleSummary {
    pub proxy_node_count: usize,
    pub routing_rule_count: usize,
    pub dokodemo_count: usize,
    pub paused_pf_count: usize,
    pub has_ui_metadata: bool,
    pub proxy_mode: Option<ProxyMode>,
    pub has_pac: bool,
    pub pac_mode: Option<PacMode>,
    pub pac_domain_count: usize,
    pub ssh_host_count: usize,
    pub ssh_forward_count: usize,
    pub absolute_resource_paths: Vec<String>,
}

/// Extract SSH jump-host and local forward topology without exporting passwords, passphrases, or private key paths.
pub fn build_ssh_topology(state: &SshState) -> BundleSshTopology {
    let creds_by_id: BTreeMap<&str, &proxy_model::SshCredentialSummary> = state
        .credentials
        .iter()
        .map(|cred| (cred.id.as_str(), cred))
        .collect();
    let mut host_ref_by_id = BTreeMap::new();
    let mut hosts = Vec::with_capacity(state.hosts.len());
    for (idx, host) in state.hosts.iter().enumerate() {
        let ref_id = format!("h{}", idx + 1);
        host_ref_by_id.insert(host.id.as_str(), ref_id.clone());
        let credential_hint =
            creds_by_id
                .get(host.credential_id.as_str())
                .map(|cred| BundleSshCredentialHint {
                    username: cred.username.clone(),
                    auth_kind: cred.auth_kind,
                });
        hosts.push(BundleSshHost {
            ref_id,
            name: host.name.clone(),
            host: host.host.clone(),
            port: host.port,
            host_key_fingerprint: host.host_key_fingerprint.clone(),
            credential_hint,
        });
    }
    let forwards = state
        .forwards
        .iter()
        .filter_map(|fwd| {
            host_ref_by_id
                .get(fwd.ssh_host_id.as_str())
                .map(|host_ref| BundleSshForward {
                    name: fwd.name.clone(),
                    listen: fwd.listen.clone(),
                    port: fwd.port,
                    target_host: fwd.target_host.clone(),
                    target_port: fwd.target_port,
                    host_ref: host_ref.clone(),
                })
        })
        .collect();
    BundleSshTopology { hosts, forwards }
}

fn derive_key(
    passphrase: &str,
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<[u8; 32]> {
    if !(8..=MAX_M_COST).contains(&m_cost)
        || !(1..=MAX_T_COST).contains(&t_cost)
        || !(1..=MAX_P_COST).contains(&p_cost)
        || m_cost < p_cost * 8
    {
        return Err(Error::new(
            "BUNDLE_UNSUPPORTED_KDF",
            "配置包加密参数超出允许的安全范围",
        ));
    }
    let params = Params::new(m_cost, t_cost, p_cost, Some(32))
        .map_err(|e| Error::new("BUNDLE_KDF_INVALID", format!("无效的加密参数：{e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| Error::new("BUNDLE_KDF_FAILED", format!("口令密钥派生失败：{e}")))?;
    Ok(key)
}

pub fn encode_bundle_envelope(
    payload: &BundlePayload,
    passphrase: Option<&str>,
    app_version: &str,
    exported_at_utc: &str,
) -> Result<BundleEnvelope> {
    if let Some(secret) = passphrase {
        if secret.is_empty() {
            return Err(Error::new("BUNDLE_PASSPHRASE_EMPTY", "加密口令不能为空"));
        }
        let mut salt = [0u8; 16];
        let mut nonce_bytes = [0u8; 12];
        let mut rng = rand::rng();
        rng.fill_bytes(&mut salt);
        rng.fill_bytes(&mut nonce_bytes);

        let key = derive_key(
            secret,
            &salt,
            DEFAULT_M_COST,
            DEFAULT_T_COST,
            DEFAULT_P_COST,
        )?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| Error::new("BUNDLE_ENCRYPT_FAILED", "初始化加密器失败"))?;
        let plaintext = serde_json::to_vec(payload)?;
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: BUNDLE_AAD,
                },
            )
            .map_err(|_| Error::new("BUNDLE_ENCRYPT_FAILED", "配置包加密失败"))?;
        let b64 = &base64::engine::general_purpose::STANDARD_NO_PAD;
        Ok(BundleEnvelope {
            format: BUNDLE_FORMAT.into(),
            schema_version: BUNDLE_SCHEMA_VERSION,
            exported_at_utc: exported_at_utc.into(),
            app_version: app_version.into(),
            encryption: BundleEncryption::Argon2idAes256Gcm {
                m_cost: DEFAULT_M_COST,
                t_cost: DEFAULT_T_COST,
                p_cost: DEFAULT_P_COST,
                salt: b64.encode(salt),
                nonce: b64.encode(nonce_bytes),
                ciphertext: b64.encode(ciphertext),
            },
            payload: None,
        })
    } else {
        Ok(BundleEnvelope {
            format: BUNDLE_FORMAT.into(),
            schema_version: BUNDLE_SCHEMA_VERSION,
            exported_at_utc: exported_at_utc.into(),
            app_version: app_version.into(),
            encryption: BundleEncryption::None,
            payload: Some(payload.clone()),
        })
    }
}

pub fn encode_bundle_json(
    payload: &BundlePayload,
    passphrase: Option<&str>,
    app_version: &str,
    exported_at_utc: &str,
) -> Result<String> {
    let envelope = encode_bundle_envelope(payload, passphrase, app_version, exported_at_utc)?;
    Ok(serde_json::to_string_pretty(&envelope)?)
}

pub fn encode_bundle_code(
    payload: &BundlePayload,
    passphrase: Option<&str>,
    app_version: &str,
    exported_at_utc: &str,
) -> Result<String> {
    let envelope = encode_bundle_envelope(payload, passphrase, app_version, exported_at_utc)?;
    let compact = serde_json::to_vec(&envelope)?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&compact)?;
    let compressed = encoder.finish()?;
    if compressed.len() > MAX_BUNDLE_BYTES {
        return Err(Error::new("BUNDLE_TOO_LARGE", "配置包体积超过 8 MiB 上限"));
    }
    Ok(format!(
        "{SHARE_CODE_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed)
    ))
}

pub fn is_bundle_text(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.starts_with(SHARE_CODE_PREFIX) {
        return true;
    }
    trimmed.starts_with('{')
        && serde_json::from_str::<Value>(trimmed)
            .is_ok_and(|v| v.get("format").and_then(Value::as_str) == Some(BUNDLE_FORMAT))
}

fn decode_share_code_bytes(code: &str) -> Result<Vec<u8>> {
    let encoded: String = code
        .trim()
        .strip_prefix(SHARE_CODE_PREFIX)
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if encoded.is_empty() {
        return Err(Error::new("BUNDLE_INVALID", "分享码内容为空"));
    }
    let compressed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&encoded)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(&encoded))
        .map_err(|_| Error::new("BUNDLE_INVALID", "配置包分享码 Base64 解码失败"))?;
    if compressed.len() > MAX_BUNDLE_BYTES {
        return Err(Error::new("BUNDLE_TOO_LARGE", "配置包体积超过 8 MiB 上限"));
    }
    let mut decoder = GzDecoder::new(&compressed[..]).take((MAX_BUNDLE_BYTES + 1) as u64);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|_| Error::new("BUNDLE_INVALID", "配置包分享码解压失败"))?;
    if out.len() > MAX_BUNDLE_BYTES {
        return Err(Error::new(
            "BUNDLE_TOO_LARGE",
            "配置包解压后体积超过 8 MiB 安全限制",
        ));
    }
    Ok(out)
}

pub fn parse_bundle_input(input: &str) -> Result<ParsedBundleInput> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(Error::new("BUNDLE_INVALID", "配置包内容为空"));
    }
    let json_bytes = if trimmed.starts_with(SHARE_CODE_PREFIX) {
        decode_share_code_bytes(trimmed)?
    } else {
        let bytes = trimmed.as_bytes();
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err(Error::new("BUNDLE_TOO_LARGE", "配置包体积超过 8 MiB 上限"));
        }
        bytes.to_vec()
    };
    let envelope: BundleEnvelope = serde_json::from_slice(&json_bytes)
        .map_err(|e| Error::new("BUNDLE_INVALID", format!("配置包格式无法解析：{e}")))?;
    if envelope.format != BUNDLE_FORMAT {
        return Err(Error::new(
            "BUNDLE_INVALID",
            format!("未知的配置包标识：{}", envelope.format),
        ));
    }
    if envelope.schema_version != BUNDLE_SCHEMA_VERSION {
        return Err(Error::new(
            "BUNDLE_UNSUPPORTED_VERSION",
            format!(
                "暂不支持的配置包版本 v{}（当前支持 v{BUNDLE_SCHEMA_VERSION}）",
                envelope.schema_version
            ),
        ));
    }
    match &envelope.encryption {
        BundleEncryption::None => {
            let payload = envelope
                .payload
                .clone()
                .ok_or_else(|| Error::new("BUNDLE_INVALID", "明文配置包缺少 payload 数据"))?;
            validate_payload(&payload)?;
            Ok(ParsedBundleInput::Plain(envelope, Box::new(payload)))
        }
        BundleEncryption::Argon2idAes256Gcm { .. } => Ok(ParsedBundleInput::Encrypted(envelope)),
    }
}

pub fn decrypt_bundle(envelope: &BundleEnvelope, passphrase: &str) -> Result<BundlePayload> {
    if passphrase.is_empty() {
        return Err(Error::new("BUNDLE_PASSPHRASE_EMPTY", "请输入解密口令"));
    }
    let BundleEncryption::Argon2idAes256Gcm {
        m_cost,
        t_cost,
        p_cost,
        salt,
        nonce,
        ciphertext,
    } = &envelope.encryption
    else {
        return envelope
            .payload
            .clone()
            .ok_or_else(|| Error::new("BUNDLE_INVALID", "配置包缺少 payload 数据"));
    };
    let b64 = &base64::engine::general_purpose::STANDARD_NO_PAD;
    let decode_flex = |raw: &str| {
        b64.decode(raw)
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(raw))
    };
    let salt_bytes =
        decode_flex(salt).map_err(|_| Error::new("BUNDLE_INVALID", "加密盐值格式无效"))?;
    let nonce_vec =
        decode_flex(nonce).map_err(|_| Error::new("BUNDLE_INVALID", "加密 Nonce 格式无效"))?;
    let nonce_arr: [u8; 12] = nonce_vec
        .try_into()
        .map_err(|_| Error::new("BUNDLE_INVALID", "加密 Nonce 长度无效"))?;
    let ct_bytes =
        decode_flex(ciphertext).map_err(|_| Error::new("BUNDLE_INVALID", "密文内容编码无效"))?;

    let key = derive_key(passphrase, &salt_bytes, *m_cost, *t_cost, *p_cost)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| Error::new("BUNDLE_DECRYPT_FAILED", "初始化解密器失败"))?;
    let nonce = Nonce::from(nonce_arr);
    let plaintext = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &ct_bytes,
                aad: BUNDLE_AAD,
            },
        )
        .map_err(|_| Error::new("BUNDLE_DECRYPT_FAILED", "口令不正确或配置包已被损坏/篡改"))?;
    let payload: BundlePayload = serde_json::from_slice(&plaintext)
        .map_err(|e| Error::new("BUNDLE_INVALID", format!("解密后的配置数据无效：{e}")))?;
    validate_payload(&payload)?;
    Ok(payload)
}

fn validate_payload(payload: &BundlePayload) -> Result<()> {
    if !payload.profile.config_json.is_object() {
        return Err(Error::new(
            "BUNDLE_INVALID",
            "配置包中的 config_json 必须为 JSON 对象",
        ));
    }
    if let Some(ssh) = &payload.ssh_topology {
        let host_refs: BTreeSet<&str> = ssh.hosts.iter().map(|h| h.ref_id.as_str()).collect();
        for fwd in &ssh.forwards {
            if !host_refs.contains(fwd.host_ref.as_str()) {
                return Err(Error::new(
                    "BUNDLE_INVALID",
                    format!(
                        "SSH 隧道「{}」引用了不存在的跳板机 {}",
                        fwd.name, fwd.host_ref
                    ),
                ));
            }
        }
    }
    Ok(())
}

pub fn summarize_payload(payload: &BundlePayload) -> BundleSummary {
    let config = &payload.profile.config_json;
    let proxy_node_count = config
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|item| {
                    !matches!(
                        item.get("protocol")
                            .and_then(Value::as_str)
                            .map(str::to_ascii_lowercase)
                            .as_deref(),
                        Some("freedom" | "blackhole" | "dns")
                    )
                })
                .count()
        })
        .unwrap_or(0);
    let routing_rule_count = config
        .pointer("/routing/rules")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let dokodemo_count = config
        .get("inbounds")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|item| {
                    item.get("protocol").and_then(Value::as_str) == Some("dokodemo-door")
                })
                .count()
        })
        .unwrap_or(0);
    let paused_pf_count = payload
        .ui_metadata
        .as_ref()
        .map_or(0, |m| m.paused_port_forwards.len());
    let has_ui_metadata = payload.ui_metadata.as_ref().is_some_and(|m| {
        !m.node_names.is_empty()
            || !m.port_forward_names.is_empty()
            || !m.paused_port_forwards.is_empty()
    });
    let proxy_mode = payload.proxy_policy.as_ref().map(|p| p.proxy_mode);
    let pac = payload.proxy_policy.as_ref().and_then(|p| p.pac.as_ref());
    let ssh_host_count = payload.ssh_topology.as_ref().map_or(0, |s| s.hosts.len());
    let ssh_forward_count = payload
        .ssh_topology
        .as_ref()
        .map_or(0, |s| s.forwards.len());
    BundleSummary {
        proxy_node_count,
        routing_rule_count,
        dokodemo_count,
        paused_pf_count,
        has_ui_metadata,
        proxy_mode,
        has_pac: pac.is_some(),
        pac_mode: pac.map(|p| p.mode),
        pac_domain_count: pac.map_or(0, |p| p.domains.len()),
        ssh_host_count,
        ssh_forward_count,
        absolute_resource_paths: crate::resources::absolute_resource_paths(config),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proxy_model::{SshCredentialSummary, SshForward, SshHost};
    use serde_json::json;
    use std::path::PathBuf;

    fn sample_payload() -> BundlePayload {
        BundlePayload {
            profile: BundleProfile {
                suggested_name: "公司网络".into(),
                config_json: json!({
                    "inbounds": [{"tag": "http-in", "protocol": "http", "listen": "127.0.0.1", "port": 10809}],
                    "outbounds": [
                        {"tag": "node-1", "protocol": "vless", "settings": {"vnext": [{"address": "example.com", "port": 443, "users": [{"id": "uuid-1", "encryption": "none"}]}]}},
                        {"tag": "direct", "protocol": "freedom", "settings": {}}
                    ],
                    "routing": {"rules": [{"type": "field", "domain": ["geosite:cn"], "outboundTag": "direct"}]}
                }),
            },
            ui_metadata: Some(BundleUiMetadata {
                node_names: BTreeMap::from([("node-1".into(), "东京节点".into())]),
                port_forward_names: BTreeMap::from([("pf-1".into(), "内网库".into())]),
                paused_port_forwards: vec![PausedPortForward {
                    tag: "pf-2".into(),
                    name: "备用".into(),
                    listen: "127.0.0.1".into(),
                    port: 15432,
                    network: "tcp".into(),
                    target_host: "10.0.0.2".into(),
                    target_port: 5432,
                    outbound_tag: "node-1".into(),
                    extra_inbound: None,
                }],
            }),
            proxy_policy: Some(BundleProxyPolicy {
                proxy_mode: ProxyMode::Pac,
                pac: Some(PacDraft {
                    mode: PacMode::ProxyList,
                    geosites: vec!["gfw".into()],
                    domains: vec!["example.com".into(), ".internal.corp".into()],
                    script: String::new(),
                    port: 18080,
                }),
            }),
            ssh_topology: Some(BundleSshTopology {
                hosts: vec![BundleSshHost {
                    ref_id: "h1".into(),
                    name: "堡垒机".into(),
                    host: "bastion.corp".into(),
                    port: 2222,
                    host_key_fingerprint: "SHA256:abc".into(),
                    credential_hint: Some(BundleSshCredentialHint {
                        username: "ops".into(),
                        auth_kind: SshAuthKind::PrivateKey,
                    }),
                }],
                forwards: vec![BundleSshForward {
                    name: "PG".into(),
                    listen: "127.0.0.1".into(),
                    port: 15432,
                    target_host: "10.0.1.8".into(),
                    target_port: 5432,
                    host_ref: "h1".into(),
                }],
            }),
        }
    }

    #[test]
    fn plain_bundle_json_and_code_roundtrip() {
        let payload = sample_payload();
        let json_text =
            encode_bundle_json(&payload, None, "0.1.1-rc.52", "2026-11-24T12:00:00Z").unwrap();
        assert!(is_bundle_text(&json_text));
        let ParsedBundleInput::Plain(env, parsed) = parse_bundle_input(&json_text).unwrap() else {
            panic!("expected plain bundle");
        };
        assert_eq!(env.schema_version, 1);
        assert_eq!(*parsed, payload);

        let code =
            encode_bundle_code(&payload, None, "0.1.1-rc.52", "2026-11-24T12:00:00Z").unwrap();
        assert!(code.starts_with(SHARE_CODE_PREFIX));
        assert!(is_bundle_text(&code));
        let ParsedBundleInput::Plain(_, from_code) = parse_bundle_input(&code).unwrap() else {
            panic!("expected plain bundle from code");
        };
        assert_eq!(*from_code, payload);

        let summary = summarize_payload(&from_code);
        assert_eq!(summary.proxy_node_count, 1);
        assert_eq!(summary.routing_rule_count, 1);
        assert_eq!(summary.paused_pf_count, 1);
        assert_eq!(summary.pac_domain_count, 2);
        assert_eq!(summary.ssh_host_count, 1);
        assert_eq!(summary.ssh_forward_count, 1);
    }

    #[test]
    fn encrypted_bundle_roundtrip_and_tamper_rejection() {
        let payload = sample_payload();
        let code = encode_bundle_code(
            &payload,
            Some("correct-horse-battery"),
            "0.1.1-rc.52",
            "2026-11-24T12:00:00Z",
        )
        .unwrap();
        assert!(!code.contains("example.com"));
        assert!(!code.contains("bastion.corp"));

        let ParsedBundleInput::Encrypted(env) = parse_bundle_input(&code).unwrap() else {
            panic!("expected encrypted bundle");
        };
        assert!(env.payload.is_none());
        assert_eq!(
            decrypt_bundle(&env, "wrong-pass").unwrap_err().code,
            "BUNDLE_DECRYPT_FAILED"
        );
        let decrypted = decrypt_bundle(&env, "correct-horse-battery").unwrap();
        assert_eq!(decrypted, payload);

        // Tamper ciphertext
        let mut tampered = env.clone();
        if let BundleEncryption::Argon2idAes256Gcm { ciphertext, .. } = &mut tampered.encryption {
            let mut chars: Vec<char> = ciphertext.chars().collect();
            chars[0] = if chars[0] == 'A' { 'B' } else { 'A' };
            *ciphertext = chars.into_iter().collect();
        }
        assert_eq!(
            decrypt_bundle(&tampered, "correct-horse-battery")
                .unwrap_err()
                .code,
            "BUNDLE_DECRYPT_FAILED"
        );

        // Reject excessive KDF memory parameter
        let mut huge_kdf = env;
        if let BundleEncryption::Argon2idAes256Gcm { m_cost, .. } = &mut huge_kdf.encryption {
            *m_cost = MAX_M_COST + 1024;
        }
        assert_eq!(
            decrypt_bundle(&huge_kdf, "correct-horse-battery")
                .unwrap_err()
                .code,
            "BUNDLE_UNSUPPORTED_KDF"
        );
    }

    #[test]
    fn rejects_unsupported_schema_and_zip_bomb_and_keeps_ssh_secrets_excluded() {
        let bad_version = json!({
            "format": BUNDLE_FORMAT,
            "schema_version": 99,
            "exported_at_utc": "2026-11-24T00:00:00Z",
            "app_version": "0.1.1-rc.52",
            "encryption": { "kind": "none" }
        });
        assert_eq!(
            parse_bundle_input(&bad_version.to_string())
                .unwrap_err()
                .code,
            "BUNDLE_UNSUPPORTED_VERSION"
        );

        // Build gzip payload larger than MAX_BUNDLE_BYTES
        let big = vec![b' '; MAX_BUNDLE_BYTES + 4096];
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&big).unwrap();
        let compressed = encoder.finish().unwrap();
        let bomb_code = format!(
            "{SHARE_CODE_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed)
        );
        assert_eq!(
            parse_bundle_input(&bomb_code).unwrap_err().code,
            "BUNDLE_TOO_LARGE"
        );

        // Security invariant: SSH topology never contains private_key_path or secret flags
        let ssh_state = SshState {
            credentials: vec![SshCredentialSummary {
                id: "cred-1".into(),
                name: "ProdKey".into(),
                username: "ops".into(),
                auth_kind: SshAuthKind::PrivateKey,
                has_password: false,
                private_key_path: PathBuf::from(r"C:\Users\secret\.ssh\id_ed25519_super_secret"),
                has_passphrase: true,
            }],
            hosts: vec![SshHost {
                id: "host-1".into(),
                name: "Bastion".into(),
                host: "bastion.example.com".into(),
                port: 22,
                credential_id: "cred-1".into(),
                host_key_fingerprint: "SHA256:xyz".into(),
            }],
            forwards: vec![SshForward {
                id: "fwd-1".into(),
                name: "DB".into(),
                listen: "127.0.0.1".into(),
                port: 15432,
                target_host: "10.0.0.5".into(),
                target_port: 5432,
                ssh_host_id: "host-1".into(),
            }],
            runtime: vec![],
        };
        let topo = build_ssh_topology(&ssh_state);
        let serialized = serde_json::to_string(&topo).unwrap();
        assert!(!serialized.contains("id_ed25519_super_secret"));
        assert!(!serialized.contains("private_key_path"));
        assert_eq!(
            topo.hosts[0].credential_hint.as_ref().unwrap().username,
            "ops"
        );
        assert_eq!(topo.forwards.len(), 1);
    }

    #[test]
    fn detects_unix_and_windows_absolute_resource_paths() {
        let config = json!({
            "inbounds": [{
                "protocol": "http",
                "streamSettings": {
                    "tlsSettings": {
                        "certificates": [{
                            "certificateFile": "C:\\certs\\server.pem",
                            "keyFile": "relative.key"
                        }]
                    }
                }
            }],
            "routing": {
                "rules": [{
                    "domain": ["geosite:cn", "ext:/opt/geo/custom.dat:ads"]
                }]
            }
        });
        let abs = crate::resources::absolute_resource_paths(&config);
        assert_eq!(abs, vec!["/opt/geo/custom.dat", "C:\\certs\\server.pem"]);
    }
}
