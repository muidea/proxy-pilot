//! Pure Rust (`russh`) SSH tunnel manager and local TCP `direct-tcpip` port-forwarding runtime.
use crate::ssh_store::ResolvedCredential;
use proxy_model::{
    Error, Result, SshAuthKind, SshForward, SshForwardRuntime, SshForwardState, SshHost,
    SshHostKeyProbe,
};
use proxy_runtime::Logs;
use russh::{
    Channel, MethodSet,
    client::{self, AuthResult, Handle},
    keys::{HashAlg, PrivateKeyWithHashAlg, load_secret_key},
};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::{Mutex as AsyncMutex, watch},
    task::JoinHandle,
    time::timeout,
};

const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SSH_PROBE_TIMEOUT: Duration = Duration::from_secs(6);

pub fn normalize_fingerprint(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed
        .strip_prefix("SHA256:")
        .or_else(|| trimmed.strip_prefix("sha256:"))
    {
        format!("SHA256:{}", rest.trim())
    } else if trimmed.is_empty() {
        String::new()
    } else {
        format!("SHA256:{trimmed}")
    }
}

struct ProbeHandler {
    captured: Arc<Mutex<Option<(String, String)>>>,
}

impl client::Handler for ProbeHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        let key = server_public_key.public_key();
        let fp = key.fingerprint(HashAlg::Sha256).to_string();
        let algo = key.algorithm().to_string();
        if let Ok(mut guard) = self.captured.lock() {
            *guard = Some((fp, algo));
        }
        // Reject after capturing KEX host key so probing never attempts authentication.
        Ok(false)
    }
}

pub async fn probe_host_key(host: &str, port: u16) -> Result<SshHostKeyProbe> {
    let host = host.trim();
    if host.is_empty()
        || host.len() > 255
        || host.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(Error::new("SSH_HOST_INVALID", "跳板机地址无效"));
    }
    if port == 0 {
        return Err(Error::new(
            "SSH_HOST_INVALID",
            "SSH 端口必须在 1–65535 之间",
        ));
    }
    let captured = Arc::new(Mutex::new(None));
    let handler = ProbeHandler {
        captured: Arc::clone(&captured),
    };
    let config = Arc::new(client::Config::default());
    let connect_res = timeout(
        SSH_PROBE_TIMEOUT,
        client::connect(config, (host, port), handler),
    )
    .await;
    if let Some((fingerprint, algorithm)) = captured.lock().ok().and_then(|g| g.clone()) {
        return Ok(SshHostKeyProbe {
            host: host.to_owned(),
            port,
            fingerprint,
            algorithm,
        });
    }
    match connect_res {
        Ok(Err(err)) => Err(Error::new(
            "SSH_PROBE_FAILED",
            format!("获取 SSH Host Key 指纹失败：{err}"),
        )),
        Ok(Ok(_)) => Err(Error::new("SSH_PROBE_FAILED", "未收到服务器 Host Key")),
        Err(_) => Err(Error::new(
            "SSH_PROBE_TIMEOUT",
            "连接 SSH 跳板机超时（超过 6 秒）",
        )),
    }
}

struct VerifiedClientHandler {
    expected_fingerprint: String,
    mismatch: Arc<Mutex<Option<(String, String)>>>,
}

impl client::Handler for VerifiedClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        let actual = server_public_key
            .public_key()
            .fingerprint(HashAlg::Sha256)
            .to_string();
        let ok =
            normalize_fingerprint(&actual) == normalize_fingerprint(&self.expected_fingerprint);
        if !ok && let Ok(mut slot) = self.mismatch.lock() {
            *slot = Some((self.expected_fingerprint.clone(), actual));
        }
        Ok(ok)
    }
}

async fn establish_session(
    host: &SshHost,
    cred: &ResolvedCredential,
) -> Result<Arc<Handle<VerifiedClientHandler>>> {
    let expected = normalize_fingerprint(&host.host_key_fingerprint);
    if expected.is_empty() {
        return Err(Error::new(
            "SSH_HOST_KEY_REQUIRED",
            format!(
                "跳板机「{}」尚未配置服务器 Host Key 指纹，请先探测并确认指纹",
                host.name
            ),
        ));
    }

    let mismatch = Arc::new(Mutex::new(None));
    let handler = VerifiedClientHandler {
        expected_fingerprint: expected,
        mismatch: Arc::clone(&mismatch),
    };
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        inactivity_timeout: None,
        ..Default::default()
    });

    let mut session = timeout(
        SSH_CONNECT_TIMEOUT,
        client::connect(config, (host.host.as_str(), host.port), handler),
    )
    .await
    .map_err(|_| Error::new("SSH_CONNECT_TIMEOUT", "连接 SSH 跳板机超时"))?
    .map_err(|err| {
        if let Some((expected, actual)) = mismatch.lock().ok().and_then(|g| g.clone()) {
            return Error::new(
                "SSH_HOST_KEY_MISMATCH",
                format!("服务器 Host Key 指纹不匹配（期望 {expected}，实际 {actual}），已拒绝连接"),
            );
        }
        Error::new("SSH_CONNECT_FAILED", format!("SSH 握手连接失败：{err}"))
    })?;

    let auth_res = match cred.auth_kind {
        SshAuthKind::Password => {
            let password = cred.password.as_deref().ok_or_else(|| {
                Error::new("SSH_AUTH_MISSING", "SSH 凭证缺少密码，请重新保存凭证")
            })?;
            session
                .authenticate_password(&cred.username, password)
                .await
                .map_err(|e| Error::new("SSH_AUTH_FAILED", format!("SSH 密码认证失败：{e}")))?
        }
        SshAuthKind::PrivateKey => {
            let key = load_secret_key(&cred.private_key_path, cred.passphrase.as_deref())
                .map_err(|e| Error::new("SSH_KEY_INVALID", format!("加载 SSH 私钥失败：{e}")))?;
            let rsa_hash = if key.algorithm().is_rsa() {
                session
                    .best_supported_rsa_hash()
                    .await
                    .ok()
                    .flatten()
                    .flatten()
            } else {
                None
            };
            session
                .authenticate_publickey(
                    &cred.username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash),
                )
                .await
                .map_err(|e| Error::new("SSH_AUTH_FAILED", format!("SSH 私钥认证失败：{e}")))?
        }
    };

    match auth_res {
        AuthResult::Success => Ok(Arc::new(session)),
        AuthResult::Failure {
            remaining_methods, ..
        } => Err(Error::new(
            "SSH_AUTH_REJECTED",
            format!(
                "SSH 登录认证被拒绝（账号：{}，服务器允许方式：{}）",
                cred.username,
                format_methods(&remaining_methods)
            ),
        )),
    }
}

fn format_methods(methods: &MethodSet) -> String {
    let list: Vec<&str> = methods.iter().map(<&'static str>::from).collect();
    if list.is_empty() {
        "无".into()
    } else {
        list.join(", ")
    }
}

#[derive(Clone)]
struct SessionEntry {
    fingerprint: String,
    host: String,
    port: u16,
    username: String,
    handle: Arc<Handle<VerifiedClientHandler>>,
}

struct ActiveForward {
    ssh_host_id: String,
    stop_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

pub struct SshTunnelManager {
    sessions: Arc<AsyncMutex<HashMap<String, SessionEntry>>>,
    runtimes: Arc<Mutex<HashMap<String, SshForwardRuntime>>>,
    active: HashMap<String, ActiveForward>,
}

impl Default for SshTunnelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SshTunnelManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(AsyncMutex::new(HashMap::new())),
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            active: HashMap::new(),
        }
    }

    pub fn runtime_snapshot(&self, forwards: &[SshForward]) -> Vec<SshForwardRuntime> {
        let guard = self.runtimes.lock().ok();
        forwards
            .iter()
            .map(|fwd| {
                guard
                    .as_ref()
                    .and_then(|m| m.get(&fwd.id).cloned())
                    .unwrap_or_else(|| SshForwardRuntime {
                        forward_id: fwd.id.clone(),
                        state: SshForwardState::Stopped,
                        last_error: None,
                    })
            })
            .collect()
    }

    pub fn is_running(&self, forward_id: &str) -> bool {
        self.active.contains_key(forward_id)
    }

    pub async fn start_forward(
        &mut self,
        forward: &SshForward,
        host: &SshHost,
        cred: ResolvedCredential,
        logs: &Logs,
    ) -> Result<()> {
        if self.active.contains_key(&forward.id) {
            self.stop_forward(&forward.id).await;
        }
        self.set_runtime(&forward.id, SshForwardState::Connecting, None);

        let session = match Self::get_or_connect_session(&self.sessions, host, &cred).await {
            Ok(session) => session,
            Err(err) => {
                self.set_runtime(
                    &forward.id,
                    SshForwardState::Error,
                    Some(err.message.clone()),
                );
                proxy_runtime::log(
                    logs,
                    format!(
                        "SSH 转发「{}」连接跳板机 {}:{} 失败：{}",
                        forward.name, host.host, host.port, err.message
                    ),
                );
                return Err(err);
            }
        };

        let bind_addr = format!("{}:{}", forward.listen, forward.port);
        let listener = match TcpListener::bind(&bind_addr).await {
            Ok(listener) => listener,
            Err(err) => {
                let msg = format!("无法监听本地端口 {bind_addr}：{err}");
                self.set_runtime(&forward.id, SshForwardState::Error, Some(msg.clone()));
                proxy_runtime::log(logs, format!("SSH 转发「{}」启动失败：{msg}", forward.name));
                return Err(Error::new("SSH_FORWARD_BIND_FAILED", msg));
            }
        };

        let (stop_tx, stop_rx) = watch::channel(false);
        let sessions = Arc::clone(&self.sessions);
        let runtimes = Arc::clone(&self.runtimes);
        let forward_owned = forward.clone();
        let host_owned = host.clone();
        let cred_owned = cred;
        let logs_cloned = logs.clone();
        let _keep_session_ref = session;

        let task = tokio::spawn(async move {
            run_forward_loop(
                listener,
                forward_owned,
                host_owned,
                cred_owned,
                sessions,
                runtimes,
                logs_cloned,
                stop_rx,
            )
            .await;
        });

        self.set_runtime(&forward.id, SshForwardState::Running, None);
        self.active.insert(
            forward.id.clone(),
            ActiveForward {
                ssh_host_id: host.id.clone(),
                stop_tx,
                task,
            },
        );
        proxy_runtime::log(
            logs,
            format!(
                "SSH 转发「{}」已启动：{}:{} -> {}:{} (via {}@{}:{})",
                forward.name,
                forward.listen,
                forward.port,
                forward.target_host,
                forward.target_port,
                host.host,
                host.port,
                normalize_fingerprint(&host.host_key_fingerprint),
            ),
        );
        Ok(())
    }

    pub async fn stop_forward(&mut self, forward_id: &str) {
        if let Some(entry) = self.active.remove(forward_id) {
            let _ = entry.stop_tx.send(true);
            let mut task = entry.task;
            if timeout(Duration::from_secs(2), &mut task).await.is_err() {
                task.abort();
            }
            self.prune_unused_session(&entry.ssh_host_id).await;
        }
        self.set_runtime(forward_id, SshForwardState::Stopped, None);
    }

    pub async fn stop_all(&mut self) {
        let ids: Vec<String> = self.active.keys().cloned().collect();
        for id in ids {
            self.stop_forward(&id).await;
        }
        self.sessions.lock().await.clear();
    }

    async fn prune_unused_session(&self, ssh_host_id: &str) {
        let used: HashSet<&str> = self
            .active
            .values()
            .map(|a| a.ssh_host_id.as_str())
            .collect();
        if !used.contains(ssh_host_id) {
            self.sessions.lock().await.remove(ssh_host_id);
        }
    }

    fn set_runtime(&self, forward_id: &str, state: SshForwardState, last_error: Option<String>) {
        if let Ok(mut guard) = self.runtimes.lock() {
            guard.insert(
                forward_id.to_owned(),
                SshForwardRuntime {
                    forward_id: forward_id.to_owned(),
                    state,
                    last_error,
                },
            );
        }
    }

    async fn get_or_connect_session(
        sessions: &Arc<AsyncMutex<HashMap<String, SessionEntry>>>,
        host: &SshHost,
        cred: &ResolvedCredential,
    ) -> Result<Arc<Handle<VerifiedClientHandler>>> {
        let expected_fp = normalize_fingerprint(&host.host_key_fingerprint);
        {
            let mut guard = sessions.lock().await;
            if let Some(existing) = guard.get(&host.id) {
                if !existing.handle.is_closed()
                    && existing.fingerprint == expected_fp
                    && existing.host == host.host
                    && existing.port == host.port
                    && existing.username == cred.username
                {
                    return Ok(Arc::clone(&existing.handle));
                }
                guard.remove(&host.id);
            }
        }
        let handle = establish_session(host, cred).await?;
        let mut guard = sessions.lock().await;
        guard.insert(
            host.id.clone(),
            SessionEntry {
                fingerprint: expected_fp,
                host: host.host.clone(),
                port: host.port,
                username: cred.username.clone(),
                handle: Arc::clone(&handle),
            },
        );
        Ok(handle)
    }
}

impl Drop for SshTunnelManager {
    fn drop(&mut self) {
        for (_, entry) in self.active.drain() {
            let _ = entry.stop_tx.send(true);
            entry.task.abort();
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_forward_loop(
    listener: TcpListener,
    forward: SshForward,
    host: SshHost,
    cred: ResolvedCredential,
    sessions: Arc<AsyncMutex<HashMap<String, SessionEntry>>>,
    runtimes: Arc<Mutex<HashMap<String, SshForwardRuntime>>>,
    logs: Logs,
    mut stop_rx: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer)) => {
                        let fwd = forward.clone();
                        let hst = host.clone();
                        let crd = cred.clone();
                        let sess = Arc::clone(&sessions);
                        let run = Arc::clone(&runtimes);
                        let lgs = logs.clone();
                        tokio::spawn(async move {
                            if let Err(err) = relay_connection(stream, peer, &fwd, &hst, &crd, &sess).await {
                                if let Ok(mut guard) = run.lock()
                                    && guard.get(&fwd.id).is_some_and(|r| r.state == SshForwardState::Running)
                                {
                                    guard.insert(
                                        fwd.id.clone(),
                                        SshForwardRuntime {
                                            forward_id: fwd.id.clone(),
                                            state: SshForwardState::Running,
                                            last_error: Some(err.message.clone()),
                                        },
                                    );
                                }
                                proxy_runtime::log(
                                    &lgs,
                                    format!("SSH 转发「{}」隧道通道错误：{}", fwd.name, err.message),
                                );
                            }
                        });
                    }
                    Err(err) => {
                        proxy_runtime::log(&logs, format!("SSH 转发「{}」accept 失败：{err}", forward.name));
                        break;
                    }
                }
            }
        }
    }
}

async fn relay_connection(
    mut local: TcpStream,
    peer: SocketAddr,
    forward: &SshForward,
    host: &SshHost,
    cred: &ResolvedCredential,
    sessions: &Arc<AsyncMutex<HashMap<String, SessionEntry>>>,
) -> Result<()> {
    let handle = SshTunnelManager::get_or_connect_session(sessions, host, cred).await?;
    let channel: Channel<russh::client::Msg> = match handle
        .channel_open_direct_tcpip(
            forward.target_host.clone(),
            u32::from(forward.target_port),
            peer.ip().to_string(),
            u32::from(peer.port()),
        )
        .await
    {
        Ok(ch) => ch,
        Err(_) => {
            // Drop stale cached session once and retry fresh handshake cleanly.
            sessions.lock().await.remove(&host.id);
            let fresh = SshTunnelManager::get_or_connect_session(sessions, host, cred).await?;
            fresh
                .channel_open_direct_tcpip(
                    forward.target_host.clone(),
                    u32::from(forward.target_port),
                    peer.ip().to_string(),
                    u32::from(peer.port()),
                )
                .await
                .map_err(|e| {
                    Error::new(
                        "SSH_CHANNEL_OPEN_FAILED",
                        format!(
                            "打开 SSH 转发通道 {}:{} 失败：{e}",
                            forward.target_host, forward.target_port
                        ),
                    )
                })?
        }
    };
    let mut ssh_stream = channel.into_stream();
    let _ = copy_bidirectional(&mut local, &mut ssh_stream).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::{PrivateKey, ssh_key::private::Ed25519Keypair};
    use russh::server::{self, Auth, ChannelOpenHandle, Server as _};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[derive(Clone)]
    struct MockSshServer {
        expected_user: String,
        expected_password: Option<String>,
        expected_pubkey: Option<russh::keys::PublicKey>,
    }

    impl server::Server for MockSshServer {
        type Handler = Self;
        fn new_client(&mut self, _: Option<SocketAddr>) -> Self::Handler {
            self.clone()
        }
    }

    impl server::Handler for MockSshServer {
        type Error = russh::Error;

        async fn auth_password(
            &mut self,
            user: &str,
            password: &str,
        ) -> std::result::Result<Auth, Self::Error> {
            if user == self.expected_user && self.expected_password.as_deref() == Some(password) {
                Ok(Auth::Accept)
            } else {
                Ok(Auth::Reject {
                    proceed_with_methods: None,
                    partial_success: false,
                })
            }
        }

        async fn auth_publickey(
            &mut self,
            user: &str,
            public_key: russh::server::VerifiedPublicKey<'_>,
        ) -> std::result::Result<Auth, Self::Error> {
            if user == self.expected_user
                && self.expected_pubkey.as_ref().is_some_and(|k| {
                    k.fingerprint(HashAlg::Sha256)
                        == public_key.public_key.fingerprint(HashAlg::Sha256)
                })
            {
                Ok(Auth::Accept)
            } else {
                Ok(Auth::Reject {
                    proceed_with_methods: None,
                    partial_success: false,
                })
            }
        }

        async fn channel_open_direct_tcpip(
            &mut self,
            channel: Channel<russh::server::Msg>,
            host_to_connect: &str,
            port_to_connect: u32,
            _originator_address: &str,
            _originator_port: u32,
            reply: ChannelOpenHandle,
            _session: &mut russh::server::Session,
        ) -> std::result::Result<(), Self::Error> {
            let target = format!("{host_to_connect}:{port_to_connect}");
            let Ok(mut upstream) = TcpStream::connect(&target).await else {
                return Ok(());
            };
            let _ = reply.accept().await;
            tokio::spawn(async move {
                let mut stream = channel.into_stream();
                let _ = copy_bidirectional(&mut stream, &mut upstream).await;
            });
            Ok(())
        }
    }

    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    async fn spawn_echo_target() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 64];
                    if let Ok(n) = sock.read(&mut buf).await
                        && n > 0
                    {
                        let _ = sock.write_all(&buf[..n]).await;
                    }
                });
            }
        });
        addr
    }

    #[tokio::test]
    async fn ssh_probe_fingerprint_verification_and_tcp_forward_work() {
        let host_key = PrivateKey::from(Ed25519Keypair::from_seed(&[1u8; 32]));
        let expected_fp = host_key
            .public_key()
            .fingerprint(HashAlg::Sha256)
            .to_string();
        let client_key = PrivateKey::from(Ed25519Keypair::from_seed(&[2u8; 32]));
        let tmp = tempfile::tempdir().unwrap();
        let key_path = tmp.path().join("id_ed25519");
        let openssh_pem = client_key
            .to_openssh(russh::keys::ssh_key::LineEnding::LF)
            .unwrap();
        std::fs::write(&key_path, openssh_pem.as_bytes()).unwrap();

        let ssh_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ssh_addr = ssh_listener.local_addr().unwrap();
        let server = MockSshServer {
            expected_user: "ops".into(),
            expected_password: Some("pw-123".into()),
            expected_pubkey: Some(client_key.public_key().clone()),
        };
        let srv_cfg = Arc::new(server::Config {
            keys: vec![host_key],
            ..Default::default()
        });
        tokio::spawn(async move {
            while let Ok((socket, addr)) = ssh_listener.accept().await {
                let mut s = server.clone();
                let cfg = Arc::clone(&srv_cfg);
                let handler = s.new_client(Some(addr));
                tokio::spawn(async move {
                    let _ = server::run_stream(cfg, socket, handler).await;
                });
            }
        });

        let probed = probe_host_key("127.0.0.1", ssh_addr.port()).await.unwrap();
        assert_eq!(probed.fingerprint, expected_fp);

        let echo_addr = spawn_echo_target().await;
        let logs = proxy_runtime::logs();
        let mut manager = SshTunnelManager::new();

        let fwd = SshForward {
            id: "fwd-1".into(),
            name: "echo".into(),
            listen: "127.0.0.1".into(),
            port: free_port(),
            target_host: "127.0.0.1".into(),
            target_port: echo_addr.port(),
            ssh_host_id: "host-1".into(),
        };

        // 1. Mismatched fingerprint is rejected fail-closed.
        let bad_host = SshHost {
            id: "host-1".into(),
            name: "mock".into(),
            host: "127.0.0.1".into(),
            port: ssh_addr.port(),
            credential_id: "cred-1".into(),
            host_key_fingerprint: "SHA256:wrong".into(),
        };
        let pw_cred = ResolvedCredential {
            username: "ops".into(),
            auth_kind: SshAuthKind::Password,
            password: Some("pw-123".into()),
            private_key_path: Default::default(),
            passphrase: None,
        };
        let mismatch_err = manager
            .start_forward(&fwd, &bad_host, pw_cred.clone(), &logs)
            .await
            .unwrap_err();
        assert_eq!(mismatch_err.code, "SSH_HOST_KEY_MISMATCH");

        // 2. Password auth + local TCP forwarding roundtrip.
        let good_host = SshHost {
            host_key_fingerprint: expected_fp.clone(),
            ..bad_host
        };
        manager
            .start_forward(&fwd, &good_host, pw_cred, &logs)
            .await
            .unwrap();
        let mut conn = TcpStream::connect((fwd.listen.as_str(), fwd.port))
            .await
            .unwrap();
        conn.write_all(b"hello-ssh-tunnel").await.unwrap();
        let mut reply = vec![0u8; 16];
        conn.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"hello-ssh-tunnel");
        manager.stop_forward(&fwd.id).await;
        assert!(!manager.is_running(&fwd.id));

        // 3. Private-key auth + clean stop.
        let key_cred = ResolvedCredential {
            username: "ops".into(),
            auth_kind: SshAuthKind::PrivateKey,
            password: None,
            private_key_path: key_path,
            passphrase: None,
        };
        let fwd2 = SshForward {
            port: free_port(),
            ..fwd
        };
        manager
            .start_forward(&fwd2, &good_host, key_cred, &logs)
            .await
            .unwrap();
        let mut conn2 = TcpStream::connect((fwd2.listen.as_str(), fwd2.port))
            .await
            .unwrap();
        conn2.write_all(b"key-forward").await.unwrap();
        let mut reply2 = vec![0u8; 11];
        conn2.read_exact(&mut reply2).await.unwrap();
        assert_eq!(&reply2, b"key-forward");
        manager.stop_all().await;
    }
}
