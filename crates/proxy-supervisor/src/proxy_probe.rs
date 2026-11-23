//! Non-blocking end-to-end Xray proxy availability probing, anti-flap debouncing,
//! and five-step diagnostic reporting.
use base64::Engine;
use proxy_config::geodata::GeoData;
use proxy_model::{
    DEFAULT_PROXY_PROBE_INTERVAL_SECS, DEFAULT_PROXY_PROBE_TIMEOUT_SECS, DEFAULT_PROXY_PROBE_URL,
    DiagnosticStep, DiagnosticStepStatus, Error, Generation, ObservatoryOutboundStatus,
    ProxyAvailability, ProxyAvailabilityState, ProxyMode, ProxyProbeConfig, ProxyProbeSample,
    Result, RouteContext, RouteResult,
};
use rustls_pki_types::ServerName;
use serde_json::Value;
use std::{
    collections::VecDeque,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    task::JoinHandle,
    time::timeout,
};
use tokio_rustls::TlsConnector;

const PROBE_CONFIG_REL: &str = "config/proxy-probe.json";
const MAX_HISTORY_SAMPLES: usize = 10;
const ANTIFLAP_RETRY_SECS: u64 = 3;
const UNAVAILABLE_FAST_RETRY_SECS: u64 = 15;
const UNAVAILABLE_BACKOFF_SECS: u64 = 60;
const TRAY_IDLE_AFTER_SECS: u64 = 15;
const TRAY_THROTTLED_INTERVAL_SECS: u64 = 120;
const SLOW_LATENCY_MS: u64 = 1500;
const MAX_HTTP_HEADER_BYTES: usize = 8192;

#[derive(Clone, Debug)]
pub(crate) struct ProbeInbound {
    pub tag: String,
    pub protocol: String,
    pub endpoint: SocketAddr,
    pub account: Option<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedProbeUrl {
    pub scheme_https: bool,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
    pub canonical_url: String,
}

#[derive(Clone)]
pub(crate) struct ProbeContext {
    pub generation: Generation,
    pub runtime_instance_id: String,
    pub profile_display_name: String,
    pub proxy_state: String,
    pub config: Value,
    pub geodata: GeoData,
    pub pac_script: Option<String>,
}

struct ProbeOutcome {
    generation_id: String,
    runtime_instance_id: String,
    target_url: String,
    checked_at: String,
    manual: bool,
    inbound: Option<ProbeInbound>,
    route: Option<RouteResult>,
    pac_decision: Option<(bool, String)>,
    local_ms: Option<u64>,
    tls_ms: Option<u64>,
    total_ms: Option<u64>,
    http_status: Option<u16>,
    is_local_failure: bool,
    fail_stage: Option<String>,
    fail_detail: Option<String>,
    observatory: Vec<ObservatoryOutboundStatus>,
}

struct ProbeInner {
    snapshot: ProxyAvailability,
    history: VecDeque<ProxyProbeSample>,
    context: Option<ProbeContext>,
    next_due: Option<Instant>,
    last_client_activity: Instant,
    was_tray_throttled: bool,
    in_flight_token: u64,
    in_flight_generation: Option<String>,
}

pub(crate) struct ProxyProbeManager {
    root: PathBuf,
    auto_runtime_enabled: bool,
    config: ProxyProbeConfig,
    inner: Arc<Mutex<ProbeInner>>,
    task: Option<JoinHandle<()>>,
}

impl Drop for ProxyProbeManager {
    fn drop(&mut self) {
        if let Some(handle) = self.task.take() {
            handle.abort();
        }
    }
}

pub(crate) fn sanitize_probe_config(mut config: ProxyProbeConfig) -> Result<ProxyProbeConfig> {
    let trimmed = config.target_url.trim();
    if trimmed.is_empty() {
        config.target_url = DEFAULT_PROXY_PROBE_URL.to_owned();
    } else {
        let parsed = parse_probe_url(trimmed)?;
        config.target_url = parsed.canonical_url;
    }
    if config.interval_secs == 0 {
        config.interval_secs = DEFAULT_PROXY_PROBE_INTERVAL_SECS;
    }
    config.interval_secs = config.interval_secs.clamp(10, 600);
    if config.timeout_secs == 0 {
        config.timeout_secs = DEFAULT_PROXY_PROBE_TIMEOUT_SECS;
    }
    config.timeout_secs = config.timeout_secs.clamp(2, 30);
    Ok(config)
}

pub(crate) fn parse_probe_url(raw: &str) -> Result<ParsedProbeUrl> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 || trimmed.chars().any(char::is_control) {
        return Err(Error::new(
            "PROBE_URL_INVALID",
            "探测地址不能为空且不能包含控制字符",
        ));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(Error::new("PROBE_URL_INVALID", "探测地址不能包含空格"));
    }
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let (scheme_https, default_port, rest) =
        if let Some(r) = without_fragment.strip_prefix("https://") {
            (true, 443u16, r)
        } else if let Some(r) = without_fragment.strip_prefix("http://") {
            (false, 80u16, r)
        } else {
            return Err(Error::new(
                "PROBE_URL_INVALID",
                "探测地址必须以 http:// 或 https:// 开头",
            ));
        };

    let (authority, path_and_query) = match rest.find(['/', '?']) {
        Some(idx) => {
            let (auth, tail) = rest.split_at(idx);
            let pq = if tail.starts_with('/') {
                tail.to_owned()
            } else {
                format!("/{tail}")
            };
            (auth, pq)
        }
        None => (rest, "/".to_owned()),
    };

    if authority.is_empty() || authority.contains('@') {
        return Err(Error::new(
            "PROBE_URL_INVALID",
            "探测地址必须包含有效主机名，且不支持内嵌账号",
        ));
    }

    let (host, port) = if let Some(after_bracket) = authority.strip_prefix('[') {
        let Some((ipv6, port_part)) = after_bracket.split_once(']') else {
            return Err(Error::new("PROBE_URL_INVALID", "IPv6 主机格式无效"));
        };
        if ipv6.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(Error::new("PROBE_URL_INVALID", "IPv6 地址格式无效"));
        }
        let port = if port_part.is_empty() {
            default_port
        } else if let Some(p) = port_part.strip_prefix(':') {
            p.parse::<u16>()
                .ok()
                .filter(|p| *p > 0)
                .ok_or_else(|| Error::new("PROBE_URL_INVALID", "探测端口必须是 1–65535"))?
        } else {
            return Err(Error::new("PROBE_URL_INVALID", "主机端口格式无效"));
        };
        (ipv6.to_owned(), port)
    } else if let Some((h, p)) = authority.rsplit_once(':') {
        if h.is_empty() || h.contains(':') {
            return Err(Error::new("PROBE_URL_INVALID", "主机名格式无效"));
        }
        let port = p
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or_else(|| Error::new("PROBE_URL_INVALID", "探测端口必须是 1–65535"))?;
        (h.to_owned(), port)
    } else {
        (authority.to_owned(), default_port)
    };

    if host.is_empty() {
        return Err(Error::new("PROBE_URL_INVALID", "探测主机名不能为空"));
    }

    let scheme = if scheme_https { "https" } else { "http" };
    let host_repr = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.clone()
    };
    let canonical_url = if port == default_port {
        format!("{scheme}://{host_repr}{path_and_query}")
    } else {
        format!("{scheme}://{host_repr}:{port}{path_and_query}")
    };

    Ok(ParsedProbeUrl {
        scheme_https,
        host,
        port,
        path_and_query,
        canonical_url,
    })
}

pub(crate) fn select_probe_inbound(
    config: &Value,
    preferred_http_tag: &str,
) -> Option<ProbeInbound> {
    let mut http_candidates = Vec::new();
    let mut socks_candidates = Vec::new();

    for inbound in config
        .get("inbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let protocol = inbound
            .get("protocol")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !matches!(protocol, "http" | "mixed" | "socks") {
            continue;
        }
        let network = inbound
            .pointer("/streamSettings/network")
            .and_then(Value::as_str)
            .unwrap_or("tcp");
        let security = inbound
            .pointer("/streamSettings/security")
            .and_then(Value::as_str)
            .unwrap_or("none");
        if !matches!(network, "tcp" | "raw") || security != "none" {
            continue;
        }
        let wrapped = ["tcpSettings", "rawSettings"].iter().any(|key| {
            let Some(settings) = inbound.pointer(&format!("/streamSettings/{key}")) else {
                return false;
            };
            settings
                .get("acceptProxyProtocol")
                .is_some_and(|v| v != false)
                || settings
                    .pointer("/header/type")
                    .is_some_and(|v| v != "none")
        });
        if wrapped {
            continue;
        }
        let Some(port) = inbound
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| *p > 0)
        else {
            continue;
        };
        let listen = inbound
            .get("listen")
            .and_then(Value::as_str)
            .unwrap_or("0.0.0.0");
        let Ok(ip) = listen.parse::<IpAddr>() else {
            continue;
        };
        let Some(bind_ip) = proxy_runtime::local_bind_target(ip) else {
            continue;
        };
        let tag = inbound
            .get("tag")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        let account = inbound
            .pointer("/settings/accounts")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(|a| {
                Some((
                    a.get("user")?.as_str()?.to_owned(),
                    a.get("pass")?.as_str()?.to_owned(),
                ))
            });
        let auth = inbound
            .pointer("/settings/auth")
            .and_then(Value::as_str)
            .unwrap_or("noauth");
        let account = if matches!(protocol, "socks" | "mixed") && auth == "noauth" {
            None
        } else {
            account
        };
        if matches!(protocol, "socks" | "mixed") && auth == "password" && account.is_none() {
            continue;
        }
        let entry = ProbeInbound {
            tag,
            protocol: protocol.to_owned(),
            endpoint: SocketAddr::new(bind_ip, port),
            account,
        };
        if matches!(protocol, "http" | "mixed") {
            http_candidates.push(entry);
        } else {
            socks_candidates.push(entry);
        }
    }

    if !preferred_http_tag.is_empty()
        && let Some(idx) = http_candidates
            .iter()
            .position(|c| c.tag == preferred_http_tag)
    {
        return Some(http_candidates.swap_remove(idx));
    }
    http_candidates
        .into_iter()
        .next()
        .or_else(|| socks_candidates.into_iter().next())
}

fn tls_client_config() -> Result<Arc<rustls::ClientConfig>> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let builder = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| Error::new("TLS_INIT_FAILED", format!("初始化 TLS 协议失败：{e}")))?;
    Ok(Arc::new(
        builder.with_root_certificates(roots).with_no_client_auth(),
    ))
}

async fn read_http_head<S: AsyncRead + Unpin>(stream: &mut S) -> std::io::Result<String> {
    let mut buf = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while buf.len() < MAX_HTTP_HEADER_BYTES {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            if buf.is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "连接在收到 HTTP 响应前被关闭",
                ));
            }
            break;
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") || buf.ends_with(b"\n\n") {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn parse_http_status_code(head: &str) -> Option<u16> {
    let line = head.lines().next()?.trim();
    let mut parts = line.split_whitespace();
    let proto = parts.next()?;
    if !proto.starts_with("HTTP/1.") && !proto.starts_with("HTTP/2") {
        return None;
    }
    parts.next()?.parse::<u16>().ok()
}

fn host_header_value(url: &ParsedProbeUrl) -> String {
    let default_port = if url.scheme_https { 443 } else { 80 };
    let host_part = if url.host.contains(':') {
        format!("[{}]", url.host)
    } else {
        url.host.clone()
    };
    if url.port == default_port {
        host_part
    } else {
        format!("{host_part}:{}", url.port)
    }
}

fn connect_authority(url: &ParsedProbeUrl) -> String {
    if url.host.contains(':') {
        format!("[{}]:{}", url.host, url.port)
    } else {
        format!("{}:{}", url.host, url.port)
    }
}

async fn socks5_handshake(
    stream: &mut TcpStream,
    account: Option<&(String, String)>,
    host: &str,
    port: u16,
) -> std::result::Result<(), (bool, &'static str, String)> {
    let method = if account.is_some() { 2 } else { 0 };
    stream
        .write_all(&[5, 1, method])
        .await
        .map_err(|e| (true, "本地代理入口通信失败", e.to_string()))?;
    let mut reply = [0u8; 2];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| (true, "本地代理入口握手失败", e.to_string()))?;
    if reply[0] != 5 || reply[1] != method {
        return Err((
            true,
            "本地 SOCKS5 入站握手协商失败",
            format!("返回认证方法编码 {}", reply[1]),
        ));
    }
    if let Some((user, pass)) = account {
        if user.len() > 255 || pass.len() > 255 {
            return Err((
                true,
                "本地 SOCKS5 账号密码超长",
                "用户名或密码超过 255 字节".into(),
            ));
        }
        let mut auth = Vec::with_capacity(3 + user.len() + pass.len());
        auth.push(1);
        auth.push(user.len() as u8);
        auth.extend_from_slice(user.as_bytes());
        auth.push(pass.len() as u8);
        auth.extend_from_slice(pass.as_bytes());
        stream
            .write_all(&auth)
            .await
            .map_err(|e| (true, "本地 SOCKS5 发送认证失败", e.to_string()))?;
        stream
            .read_exact(&mut reply)
            .await
            .map_err(|e| (true, "本地 SOCKS5 认证响应读取失败", e.to_string()))?;
        if reply != [1, 0] {
            return Err((
                true,
                "本地 SOCKS5 入站账号认证被拒绝",
                "请检查入站 accounts 配置".into(),
            ));
        }
    }

    let mut req = Vec::with_capacity(7 + host.len());
    req.extend_from_slice(&[5, 1, 0]);
    if let Ok(ip) = host.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(v4) => {
                req.push(1);
                req.extend_from_slice(&v4.octets());
            }
            IpAddr::V6(v6) => {
                req.push(4);
                req.extend_from_slice(&v6.octets());
            }
        }
    } else {
        if host.len() > 255 {
            return Err((false, "目标域名过长", "域名超过 255 字节".into()));
        }
        req.push(3);
        req.push(host.len() as u8);
        req.extend_from_slice(host.as_bytes());
    }
    req.extend_from_slice(&port.to_be_bytes());
    stream
        .write_all(&req)
        .await
        .map_err(|e| (true, "本地 SOCKS5 发送连接请求失败", e.to_string()))?;

    let mut head = [0u8; 4];
    stream
        .read_exact(&mut head)
        .await
        .map_err(|e| (false, "经由代理建立隧道失败", e.to_string()))?;
    if head[1] != 0 {
        let msg = match head[1] {
            1 => "常规 SOCKS 服务器错误（远端节点不可达或配置无效）",
            2 => "规则拒绝访问目标地址",
            3 => "远端网络不可达",
            4 => "远端主机不可达",
            5 => "远端拒绝连接",
            6 => "连接超时",
            _ => "SOCKS5 代理连接失败",
        };
        return Err((
            false,
            "远端代理出口拨号失败",
            format!("{msg} (code={})", head[1]),
        ));
    }
    let skip_len = match head[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut len = [0u8; 1];
            stream
                .read_exact(&mut len)
                .await
                .map_err(|e| (false, "读取 SOCKS5 响应地址失败", e.to_string()))?;
            usize::from(len[0]) + 2
        }
        other => {
            return Err((
                false,
                "SOCKS5 响应格式无效",
                format!("未知地址类型 atyp={other}"),
            ));
        }
    };
    let mut discard = vec![0u8; skip_len];
    stream
        .read_exact(&mut discard)
        .await
        .map_err(|e| (false, "读取 SOCKS5 响应尾部失败", e.to_string()))?;
    Ok(())
}

async fn send_http_get<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    request_target: &str,
    host_header: &str,
    proxy_auth: Option<&str>,
) -> std::result::Result<u16, (bool, &'static str, String)> {
    let mut req = format!(
        "GET {request_target} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: ProxyPilot-Probe/1\r\nAccept: */*\r\n"
    );
    if let Some(auth) = proxy_auth {
        req.push_str(&format!(
            "Proxy-Authorization: Basic {auth}\r\nProxy-Connection: close\r\n"
        ));
    }
    req.push_str("Connection: close\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| (false, "发送 HTTP 探测请求失败", e.to_string()))?;
    stream
        .flush()
        .await
        .map_err(|e| (false, "发送 HTTP 探测请求失败", e.to_string()))?;
    let head = read_http_head(stream)
        .await
        .map_err(|e| (false, "远端目标未返回有效响应", e.to_string()))?;
    let Some(code) = parse_http_status_code(&head) else {
        let preview = head
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>();
        return Err((
            false,
            "远端返回了非标准 HTTP 响应",
            if preview.is_empty() {
                "空响应".into()
            } else {
                preview
            },
        ));
    };
    if code == 407 {
        return Err((
            true,
            "本地 HTTP 代理入口鉴权失败 (HTTP 407)",
            "请核对本地入站 accounts 用户名与密码".into(),
        ));
    }
    if !(200..=399).contains(&code) {
        return Err((
            false,
            "远端目标返回异常 HTTP 状态码",
            format!("HTTP {code}（预期 2xx/3xx）"),
        ));
    }
    Ok(code)
}

async fn probe_via_inbound(
    inbound: &ProbeInbound,
    url: &ParsedProbeUrl,
) -> std::result::Result<(u64, Option<u64>, u64, u16), (bool, Option<u64>, &'static str, String)> {
    let started = Instant::now();
    let mut stream = TcpStream::connect(inbound.endpoint).await.map_err(|e| {
        (
            true,
            None,
            "无法连接本地代理入站端口",
            format!("{} -> {e}", inbound.endpoint),
        )
    })?;
    let _ = stream.set_nodelay(true);

    let basic_auth = inbound.account.as_ref().map(|(user, pass)| {
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
    });
    let host_header = host_header_value(url);

    if inbound.protocol == "socks" {
        socks5_handshake(&mut stream, inbound.account.as_ref(), &url.host, url.port)
            .await
            .map_err(|(local, stage, detail)| (local, None, stage, detail))?;
        let local_ms = started.elapsed().as_millis() as u64;
        if url.scheme_https {
            let tls_cfg = tls_client_config()
                .map_err(|e| (true, Some(local_ms), "初始化 TLS 组件失败", e.message))?;
            let server_name = ServerName::try_from(url.host.clone()).map_err(|e| {
                (
                    true,
                    Some(local_ms),
                    "探测目标主机名不适用于 TLS SNI",
                    e.to_string(),
                )
            })?;
            let tls_started = Instant::now();
            let mut tls_stream = TlsConnector::from(tls_cfg)
                .connect(server_name, stream)
                .await
                .map_err(|e| {
                    (
                        false,
                        Some(local_ms),
                        "经由代理 TLS 握手失败（远端节点不可用或被中断）",
                        e.to_string(),
                    )
                })?;
            let tls_ms = tls_started.elapsed().as_millis() as u64;
            let code = send_http_get(&mut tls_stream, &url.path_and_query, &host_header, None)
                .await
                .map_err(|(local, stage, detail)| (local, Some(local_ms), stage, detail))?;
            let total_ms = started.elapsed().as_millis() as u64;
            Ok((local_ms, Some(tls_ms), total_ms, code))
        } else {
            let code = send_http_get(&mut stream, &url.path_and_query, &host_header, None)
                .await
                .map_err(|(local, stage, detail)| (local, Some(local_ms), stage, detail))?;
            let total_ms = started.elapsed().as_millis() as u64;
            Ok((local_ms, None, total_ms, code))
        }
    } else if url.scheme_https {
        let authority = connect_authority(url);
        let mut connect_req = format!(
            "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nUser-Agent: ProxyPilot-Probe/1\r\n"
        );
        if let Some(auth) = basic_auth.as_deref() {
            connect_req.push_str(&format!("Proxy-Authorization: Basic {auth}\r\n"));
        }
        connect_req.push_str("\r\n");
        stream
            .write_all(connect_req.as_bytes())
            .await
            .map_err(|e| (true, None, "发送 CONNECT 隧道请求失败", e.to_string()))?;
        let head = read_http_head(&mut stream).await.map_err(|e| {
            (
                true,
                None,
                "本地 HTTP 代理入口未完成 CONNECT 握手",
                e.to_string(),
            )
        })?;
        let connect_code = parse_http_status_code(&head).ok_or_else(|| {
            (
                true,
                None,
                "本地 HTTP 代理入口返回无效响应",
                head.lines().next().unwrap_or("").to_owned(),
            )
        })?;
        if connect_code == 407 {
            return Err((
                true,
                None,
                "本地 HTTP 代理入口鉴权失败 (HTTP 407)",
                "请核对本地入站 accounts 用户名与密码".into(),
            ));
        }
        if connect_code != 200 {
            return Err((
                true,
                None,
                "本地 HTTP 代理拒绝 CONNECT 隧道",
                format!("HTTP {connect_code}"),
            ));
        }
        let local_ms = started.elapsed().as_millis() as u64;
        // Xray returns HTTP 200 before dialing upstream; completing TLS proves real remote flow.
        let tls_cfg = tls_client_config()
            .map_err(|e| (true, Some(local_ms), "初始化 TLS 组件失败", e.message))?;
        let server_name = ServerName::try_from(url.host.clone()).map_err(|e| {
            (
                true,
                Some(local_ms),
                "探测目标主机名不适用于 TLS SNI",
                e.to_string(),
            )
        })?;
        let tls_started = Instant::now();
        let mut tls_stream = TlsConnector::from(tls_cfg)
            .connect(server_name, stream)
            .await
            .map_err(|e| {
                (
                    false,
                    Some(local_ms),
                    "经由代理 TLS 握手失败（远端节点不可达、认证异常或连接被关闭）",
                    e.to_string(),
                )
            })?;
        let tls_ms = tls_started.elapsed().as_millis() as u64;
        let code = send_http_get(&mut tls_stream, &url.path_and_query, &host_header, None)
            .await
            .map_err(|(local, stage, detail)| (local, Some(local_ms), stage, detail))?;
        let total_ms = started.elapsed().as_millis() as u64;
        Ok((local_ms, Some(tls_ms), total_ms, code))
    } else {
        let local_ms = started.elapsed().as_millis() as u64;
        let code = send_http_get(
            &mut stream,
            &url.canonical_url,
            &host_header,
            basic_auth.as_deref(),
        )
        .await
        .map_err(|(local, stage, detail)| (local, Some(local_ms), stage, detail))?;
        let total_ms = started.elapsed().as_millis() as u64;
        Ok((local_ms, None, total_ms, code))
    }
}

pub(crate) fn parse_observatory_vars(body: &str) -> Vec<ObservatoryOutboundStatus> {
    let Ok(json) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let Some(obs) = json.get("observatory").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut items = Vec::with_capacity(obs.len());
    for (key, entry) in obs {
        let tag = entry
            .get("outbound_tag")
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_owned();
        let alive = entry.get("alive").and_then(Value::as_bool).unwrap_or(false);
        let delay_ms = entry.get("delay").and_then(Value::as_u64).filter(|_| alive);
        let last_error = entry
            .get("last_error_reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        items.push(ObservatoryOutboundStatus {
            outbound_tag: tag,
            alive,
            delay_ms,
            last_error,
        });
    }
    items.sort_by(|a, b| a.outbound_tag.cmp(&b.outbound_tag));
    items
}

async fn fetch_optional_observatory(config: &Value) -> Vec<ObservatoryOutboundStatus> {
    let Some(listen) = config.pointer("/metrics/listen").and_then(Value::as_str) else {
        return Vec::new();
    };
    let Ok(addr) = listen.trim().parse::<SocketAddr>() else {
        return Vec::new();
    };
    let Some(target_ip) = proxy_runtime::local_bind_target(addr.ip()) else {
        return Vec::new();
    };
    let endpoint = SocketAddr::new(target_ip, addr.port());
    let fut = async {
        let mut stream = TcpStream::connect(endpoint).await.ok()?;
        let req =
            format!("GET /debug/vars HTTP/1.1\r\nHost: {endpoint}\r\nConnection: close\r\n\r\n");
        stream.write_all(req.as_bytes()).await.ok()?;
        let mut raw = Vec::new();
        let _ = stream.take(256 * 1024).read_to_end(&mut raw).await.ok()?;
        let text = String::from_utf8_lossy(&raw);
        let (_, body) = text
            .split_once("\r\n\r\n")
            .or_else(|| text.split_once("\n\n"))?;
        Some(parse_observatory_vars(body))
    };
    timeout(Duration::from_millis(800), fut)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn short_timestamp_hms(full: &str) -> String {
    full.split_whitespace()
        .nth(1)
        .map(|t| t.split('.').next().unwrap_or(t).to_owned())
        .unwrap_or_else(|| full.to_owned())
}

async fn run_probe_once(
    ctx: ProbeContext,
    target_url: String,
    timeout_secs: u64,
    manual: bool,
) -> ProbeOutcome {
    let checked_at = proxy_runtime::local_timestamp();
    let parsed_url = match parse_probe_url(&target_url) {
        Ok(u) => u,
        Err(err) => {
            return ProbeOutcome {
                generation_id: ctx.generation.id.clone(),
                runtime_instance_id: ctx.runtime_instance_id,
                target_url,
                checked_at,
                manual,
                inbound: None,
                route: None,
                pac_decision: None,
                local_ms: None,
                tls_ms: None,
                total_ms: None,
                http_status: None,
                is_local_failure: true,
                fail_stage: Some("探测地址格式无效".into()),
                fail_detail: Some(err.message),
                observatory: Vec::new(),
            };
        }
    };

    let inbound = select_probe_inbound(&ctx.config, &ctx.generation.http_inbound_tag);
    let route = Some(proxy_config::routing::simulate_with_geo(
        &ctx.config,
        &RouteContext {
            host: parsed_url.host.clone(),
            port: Some(parsed_url.port),
            inbound_tag: inbound.as_ref().map(|i| i.tag.clone()),
            network: Some("tcp".into()),
            target_ip: parsed_url
                .host
                .parse::<IpAddr>()
                .ok()
                .map(|ip| ip.to_string()),
            ..Default::default()
        },
        Some(&ctx.geodata),
    ));

    let pac_decision = evaluate_pac_decision(&ctx, &parsed_url.canonical_url);

    let Some(inbound_ref) = inbound.clone() else {
        let observatory = fetch_optional_observatory(&ctx.config).await;
        return ProbeOutcome {
            generation_id: ctx.generation.id,
            runtime_instance_id: ctx.runtime_instance_id,
            target_url: parsed_url.canonical_url,
            checked_at,
            manual,
            inbound: None,
            route,
            pac_decision,
            local_ms: None,
            tls_ms: None,
            total_ms: None,
            http_status: None,
            is_local_failure: true,
            fail_stage: Some("缺少可用本地代理入站".into()),
            fail_detail: Some("未在运行配置中找到回环 http / mixed / socks 入站".into()),
            observatory,
        };
    };

    let timeout_dur = Duration::from_secs(timeout_secs.max(2));
    let probe_res = timeout(timeout_dur, probe_via_inbound(&inbound_ref, &parsed_url)).await;
    let observatory = fetch_optional_observatory(&ctx.config).await;

    match probe_res {
        Ok(Ok((local_ms, tls_ms, total_ms, code))) => ProbeOutcome {
            generation_id: ctx.generation.id,
            runtime_instance_id: ctx.runtime_instance_id,
            target_url: parsed_url.canonical_url,
            checked_at,
            manual,
            inbound: Some(inbound_ref),
            route,
            pac_decision,
            local_ms: Some(local_ms),
            tls_ms,
            total_ms: Some(total_ms),
            http_status: Some(code),
            is_local_failure: false,
            fail_stage: None,
            fail_detail: None,
            observatory,
        },
        Ok(Err((is_local, local_ms, stage, detail))) => ProbeOutcome {
            generation_id: ctx.generation.id,
            runtime_instance_id: ctx.runtime_instance_id,
            target_url: parsed_url.canonical_url,
            checked_at,
            manual,
            inbound: Some(inbound_ref),
            route,
            pac_decision,
            local_ms,
            tls_ms: None,
            total_ms: Some(timeout_secs * 1000),
            http_status: None,
            is_local_failure: is_local,
            fail_stage: Some(stage.into()),
            fail_detail: Some(detail),
            observatory,
        },
        Err(_) => ProbeOutcome {
            generation_id: ctx.generation.id,
            runtime_instance_id: ctx.runtime_instance_id,
            target_url: parsed_url.canonical_url,
            checked_at,
            manual,
            inbound: Some(inbound_ref),
            route,
            pac_decision,
            local_ms: None,
            tls_ms: None,
            total_ms: Some(timeout_secs * 1000),
            http_status: None,
            is_local_failure: false,
            fail_stage: Some("经由代理端到端探测超时".into()),
            fail_detail: Some(format!("超过 {} 秒未收到完整响应", timeout_secs)),
            observatory,
        },
    }
}

fn evaluate_pac_decision(ctx: &ProbeContext, target_url: &str) -> Option<(bool, String)> {
    if ctx.generation.proxy_mode != ProxyMode::Pac {
        return None;
    }
    ctx.pac_script
        .as_deref()
        .and_then(|script| proxy_pac::evaluate(script, target_url).ok())
        .map(|d| (d.kind.eq_ignore_ascii_case("DIRECT"), d.raw))
}

fn build_pac_step5(ctx: &ProbeContext, pac_eval: Option<(bool, String)>) -> DiagnosticStep {
    let owned = ctx.proxy_state == "Owned";
    match (ctx.generation.proxy_mode, owned) {
        (ProxyMode::Pac, true) => {
            let (pac_direct, raw) = pac_eval.map(|(d, r)| (d, Some(r))).unwrap_or((false, None));
            DiagnosticStep {
                index: 5,
                name: "系统代理 / PAC 生效关联".into(),
                status: if pac_direct {
                    DiagnosticStepStatus::Warn
                } else {
                    DiagnosticStepStatus::Pass
                },
                label: if pac_direct {
                    "PAC 返回直连".into()
                } else {
                    "PAC 已接管".into()
                },
                detail: "当前生效 PAC FindProxyForURL 评估结果".into(),
                evidence: raw,
            }
        }
        (ProxyMode::Global, true) => DiagnosticStep {
            index: 5,
            name: "系统代理 / PAC 生效关联".into(),
            status: DiagnosticStepStatus::Pass,
            label: "全局已接管".into(),
            detail: format!("系统代理指向本地入口 {}", ctx.generation.http_inbound_tag),
            evidence: None,
        },
        _ => DiagnosticStep {
            index: 5,
            name: "系统代理 / PAC 生效关联".into(),
            status: DiagnosticStepStatus::Pass,
            label: "系统代理直连".into(),
            detail: "系统代理未接管，Xray 本地入口保持独立可用".into(),
            evidence: None,
        },
    }
}

fn build_steps(
    ctx: &ProbeContext,
    outcome: Option<&ProbeOutcome>,
    target_url: &str,
    checking: bool,
) -> Vec<DiagnosticStep> {
    let gen_short: String = ctx.generation.id.chars().take(18).collect();
    let step1 = DiagnosticStep {
        index: 1,
        name: "Core 进程与运行代次".into(),
        status: DiagnosticStepStatus::Pass,
        label: "通过".into(),
        detail: format!("配置「{}」运行中 · 冻结代次", ctx.profile_display_name),
        evidence: Some(gen_short),
    };

    let inbound = outcome
        .and_then(|o| o.inbound.as_ref())
        .cloned()
        .or_else(|| select_probe_inbound(&ctx.config, &ctx.generation.http_inbound_tag));

    let step2 = if let Some(ref ib) = inbound {
        let auth_note = if ib.account.is_some() {
            "含入站认证"
        } else {
            "无认证"
        };
        if let Some(o) = outcome {
            if let Some(local_ms) = o.local_ms {
                DiagnosticStep {
                    index: 2,
                    name: "本地代理入站握手".into(),
                    status: DiagnosticStepStatus::Pass,
                    label: format!("通过 · {local_ms} ms"),
                    detail: format!(
                        "入站 Tag：{} ({}) · {auth_note}",
                        ib.tag,
                        ib.protocol.to_ascii_uppercase()
                    ),
                    evidence: Some(ib.endpoint.to_string()),
                }
            } else if o.is_local_failure {
                DiagnosticStep {
                    index: 2,
                    name: "本地代理入站握手".into(),
                    status: DiagnosticStepStatus::Fail,
                    label: "失败".into(),
                    detail: o
                        .fail_stage
                        .clone()
                        .unwrap_or_else(|| "本地入口异常".into()),
                    evidence: o
                        .fail_detail
                        .clone()
                        .or_else(|| Some(ib.endpoint.to_string())),
                }
            } else {
                DiagnosticStep {
                    index: 2,
                    name: "本地代理入站握手".into(),
                    status: DiagnosticStepStatus::Pass,
                    label: "已就绪".into(),
                    detail: format!(
                        "入站 Tag：{} ({}) · {auth_note}",
                        ib.tag,
                        ib.protocol.to_ascii_uppercase()
                    ),
                    evidence: Some(ib.endpoint.to_string()),
                }
            }
        } else {
            DiagnosticStep {
                index: 2,
                name: "本地代理入站握手".into(),
                status: if checking {
                    DiagnosticStepStatus::Checking
                } else {
                    DiagnosticStepStatus::Skipped
                },
                label: if checking {
                    "握手中…"
                } else {
                    "已选择"
                }
                .into(),
                detail: format!(
                    "入站 Tag：{} ({}) · {auth_note}",
                    ib.tag,
                    ib.protocol.to_ascii_uppercase()
                ),
                evidence: Some(ib.endpoint.to_string()),
            }
        }
    } else {
        DiagnosticStep {
            index: 2,
            name: "本地代理入站握手".into(),
            status: DiagnosticStepStatus::Warn,
            label: "无法检测".into(),
            detail: "未在运行配置中找到回环 http / mixed / socks 入站".into(),
            evidence: None,
        }
    };

    let route = outcome.and_then(|o| o.route.as_ref()).cloned().or_else(|| {
        parse_probe_url(target_url).ok().map(|parsed| {
            proxy_config::routing::simulate_with_geo(
                &ctx.config,
                &RouteContext {
                    host: parsed.host,
                    port: Some(parsed.port),
                    inbound_tag: inbound.as_ref().map(|i| i.tag.clone()),
                    network: Some("tcp".into()),
                    target_ip: None,
                    ..Default::default()
                },
                Some(&ctx.geodata),
            )
        })
    });

    let step3 = if inbound.is_none() {
        DiagnosticStep {
            index: 3,
            name: "路由规则归属（静态预测）".into(),
            status: DiagnosticStepStatus::Skipped,
            label: "跳过".into(),
            detail: "缺少本地代理入口".into(),
            evidence: None,
        }
    } else if let Some(r) = route {
        let out = r.expected_outbound.as_deref();
        let is_direct = out == Some("direct");
        let is_block = out == Some("block");
        let (status, label) = if is_direct {
            (DiagnosticStepStatus::Warn, "警告 · 命中直连".to_owned())
        } else if is_block {
            (DiagnosticStepStatus::Fail, "拦截 · 命中 block".to_owned())
        } else if let Some(tag) = out {
            (DiagnosticStepStatus::Pass, format!("通过 · {tag}"))
        } else {
            (DiagnosticStepStatus::Warn, "预测未知".to_owned())
        };
        let rule_note = r
            .rule_index
            .map(|idx| format!("命中第 {} 条规则：{}", idx + 1, r.reason))
            .unwrap_or(r.reason);
        DiagnosticStep {
            index: 3,
            name: "路由规则归属（静态预测）".into(),
            status,
            label,
            detail: if is_direct {
                "探测地址按规则直连（未走代理节点），请改用走代理的地址验证节点可用性".into()
            } else {
                rule_note
            },
            evidence: Some(format!("expected_outbound={}", out.unwrap_or("Unknown"))),
        }
    } else {
        DiagnosticStep {
            index: 3,
            name: "路由规则归属（静态预测）".into(),
            status: DiagnosticStepStatus::Skipped,
            label: "跳过".into(),
            detail: "探测地址无法解析".into(),
            evidence: None,
        }
    };

    let is_https = target_url.trim().starts_with("https://");
    let scheme_name = match inbound.as_ref().map(|i| i.protocol.as_str()) {
        Some("socks") if is_https => "SOCKS5 + TLS",
        Some("socks") => "SOCKS5 明文代理",
        _ if is_https => "HTTP CONNECT + TLS",
        _ => "HTTP 明文代理",
    };

    let step4 = if let Some(o) = outcome {
        if let Some(code) = o.http_status {
            let total_ms = o.total_ms.unwrap_or(0);
            let is_direct = o
                .route
                .as_ref()
                .and_then(|r| r.expected_outbound.as_deref())
                == Some("direct");
            let breakdown = match (o.local_ms, o.tls_ms) {
                (Some(loc), Some(tls)) => {
                    format!(
                        "{scheme_name} · HTTP {code}（本地 {loc} ms · TLS {tls} ms · 总耗时 {total_ms} ms）"
                    )
                }
                _ => format!("{scheme_name} · HTTP {code}（总耗时 {total_ms} ms）"),
            };
            DiagnosticStep {
                index: 4,
                name: "经由代理端到端连通".into(),
                status: if is_direct || total_ms > SLOW_LATENCY_MS {
                    DiagnosticStepStatus::Warn
                } else {
                    DiagnosticStepStatus::Pass
                },
                label: if is_direct {
                    format!("直连可达 · {total_ms} ms")
                } else if total_ms > SLOW_LATENCY_MS {
                    format!("响应偏慢 · {total_ms} ms")
                } else {
                    format!("可用 · {total_ms} ms")
                },
                detail: breakdown,
                evidence: Some(format!("target={}", o.target_url)),
            }
        } else {
            DiagnosticStep {
                index: 4,
                name: "经由代理端到端连通".into(),
                status: if o.inbound.is_none() {
                    DiagnosticStepStatus::Skipped
                } else {
                    DiagnosticStepStatus::Fail
                },
                label: if o.inbound.is_none() {
                    "跳过".into()
                } else {
                    "失败".into()
                },
                detail: o
                    .fail_detail
                    .clone()
                    .or_else(|| o.fail_stage.clone())
                    .unwrap_or_else(|| "未完成探测".into()),
                evidence: Some(format!("target={}", o.target_url)),
            }
        }
    } else {
        DiagnosticStep {
            index: 4,
            name: "经由代理端到端连通".into(),
            status: if checking {
                DiagnosticStepStatus::Checking
            } else {
                DiagnosticStepStatus::Skipped
            },
            label: if checking {
                "探测中…"
            } else {
                "待检测"
            }
            .into(),
            detail: format!("目标：{target_url}"),
            evidence: None,
        }
    };

    let pac_eval = outcome
        .and_then(|o| o.pac_decision.clone())
        .or_else(|| evaluate_pac_decision(ctx, target_url));
    let step5 = build_pac_step5(ctx, pac_eval);

    vec![step1, step2, step3, step4, step5]
}

impl ProxyProbeManager {
    pub(crate) fn load(root: &Path, auto_runtime_enabled: bool) -> Result<Self> {
        let raw_cfg: Option<ProxyProbeConfig> =
            proxy_config::optional_json(&root.join(PROBE_CONFIG_REL))?;
        let config = match raw_cfg {
            Some(c) => sanitize_probe_config(c).unwrap_or_default(),
            None => ProxyProbeConfig::default(),
        };
        let initial = ProxyAvailability {
            state: ProxyAvailabilityState::Stopped,
            badge_text: "未运行".into(),
            headline: "Core 尚未启动，启动后将自动检测代理可用性".into(),
            target_url: config.target_url.clone(),
            config: config.clone(),
            ..Default::default()
        };
        Ok(Self {
            root: root.to_path_buf(),
            auto_runtime_enabled,
            config,
            inner: Arc::new(Mutex::new(ProbeInner {
                snapshot: initial,
                history: VecDeque::with_capacity(MAX_HISTORY_SAMPLES),
                context: None,
                next_due: None,
                last_client_activity: Instant::now(),
                was_tray_throttled: false,
                in_flight_token: 0,
                in_flight_generation: None,
            })),
            task: None,
        })
    }

    pub(crate) fn snapshot(&self) -> ProxyAvailability {
        let mut inner = self.inner.lock().unwrap();
        let tray_throttled = self.auto_runtime_enabled
            && self.config.enabled
            && self.config.tray_throttle
            && inner.last_client_activity.elapsed() >= Duration::from_secs(TRAY_IDLE_AFTER_SECS);
        inner.snapshot.tray_throttled = tray_throttled;
        inner.snapshot.clone()
    }

    pub(crate) fn note_client_activity(&mut self) {
        let wake_probe = {
            let mut inner = self.inner.lock().unwrap();
            inner.last_client_activity = Instant::now();
            let wake = inner.was_tray_throttled
                && self.auto_runtime_enabled
                && self.config.enabled
                && inner.context.is_some();
            if wake {
                inner.was_tray_throttled = false;
                inner.next_due = Some(Instant::now());
            }
            wake
        };
        if wake_probe {
            self.tick();
        }
    }

    pub(crate) fn save_config(&mut self, config: ProxyProbeConfig) -> Result<ProxyAvailability> {
        let clean = sanitize_probe_config(config)?;
        proxy_config::write_json(&self.root.join(PROBE_CONFIG_REL), &clean)?;
        self.config = clean.clone();
        let should_schedule = {
            let mut inner = self.inner.lock().unwrap();
            inner.snapshot.config = clean.clone();
            inner.snapshot.target_url = clean.target_url.clone();
            if !clean.enabled {
                inner.next_due = None;
                false
            } else if self.auto_runtime_enabled && inner.context.is_some() {
                inner.next_due = Some(Instant::now() + Duration::from_secs(clean.interval_secs));
                true
            } else {
                false
            }
        };
        let _ = should_schedule;
        Ok(self.snapshot())
    }

    pub(crate) fn sync_context(&mut self, ctx: ProbeContext, trigger_initial: bool) {
        let generation_id = ctx.generation.id.clone();
        let runtime_id = ctx.runtime_instance_id.clone();
        let inbound_opt = select_probe_inbound(&ctx.config, &ctx.generation.http_inbound_tag);

        let generation_changed = {
            let mut inner = self.inner.lock().unwrap();
            let changed = inner.snapshot.generation_id.as_deref() != Some(&generation_id)
                || inner.snapshot.runtime_instance_id.as_deref() != Some(&runtime_id);
            inner.context = Some(ctx.clone());
            if changed {
                inner.history.clear();
                inner.in_flight_token = inner.in_flight_token.wrapping_add(1);
                inner.in_flight_generation = None;
                let mut snap = ProxyAvailability {
                    generation_id: Some(generation_id.clone()),
                    runtime_instance_id: Some(runtime_id.clone()),
                    target_url: self.config.target_url.clone(),
                    config: self.config.clone(),
                    inbound_tag: inbound_opt.as_ref().map(|i| i.tag.clone()),
                    inbound_protocol: inbound_opt.as_ref().map(|i| i.protocol.clone()),
                    ..Default::default()
                };
                if inbound_opt.is_none() {
                    snap.state = ProxyAvailabilityState::Unsupported;
                    snap.badge_text = "无法检测".into();
                    snap.overview_hint = "当前配置无可用本地代理入口".into();
                    snap.headline =
                        "当前配置不包含本机回环 HTTP / Mixed / SOCKS 入站，无法通过代理入站发起自动探测"
                            .into();
                    snap.suggestions = vec![
                        "如需浏览器代理与可用性探测，请在「配置 → 更多与源码 → 本地入口」补充回环 HTTP 或 Mixed 入站。".into(),
                        "如仅使用端口转发或 SSH 隧道，此状态为正常现象。".into(),
                    ];
                } else {
                    snap.state = ProxyAvailabilityState::Untested;
                    snap.badge_text = "待检测".into();
                    snap.headline = "Core 已就绪，正在等待代理可用性探测".into();
                }
                snap.steps = build_steps(&ctx, None, &self.config.target_url, false);
                inner.snapshot = snap;
                if self.auto_runtime_enabled
                    && self.config.enabled
                    && inbound_opt.is_some()
                    && trigger_initial
                {
                    inner.next_due = Some(Instant::now() + Duration::from_millis(300));
                } else {
                    inner.next_due = None;
                }
            } else {
                // Keep probe results valid when only proxy mode/PAC changed, refreshing step 5 + overview hint.
                let is_owned = ctx.proxy_state == "Owned"
                    && matches!(
                        ctx.generation.proxy_mode,
                        ProxyMode::Global | ProxyMode::Pac
                    );
                if inner.snapshot.state == ProxyAvailabilityState::Available {
                    inner.snapshot.overview_hint = if is_owned {
                        String::new()
                    } else {
                        "当前系统代理为直连，流量暂未接管".into()
                    };
                }
                let pac_eval = evaluate_pac_decision(&ctx, &inner.snapshot.target_url);
                if let Some(step5) = inner.snapshot.steps.iter_mut().find(|s| s.index == 5) {
                    *step5 = build_pac_step5(&ctx, pac_eval);
                }
            }
            changed
        };
        if generation_changed && let Some(handle) = self.task.take() {
            handle.abort();
        }
    }

    pub(crate) fn on_core_stopped(&mut self) {
        if let Some(handle) = self.task.take() {
            handle.abort();
        }
        let mut inner = self.inner.lock().unwrap();
        inner.in_flight_token = inner.in_flight_token.wrapping_add(1);
        inner.in_flight_generation = None;
        inner.context = None;
        inner.next_due = None;
        inner.history.clear();
        inner.snapshot = ProxyAvailability {
            state: ProxyAvailabilityState::Stopped,
            badge_text: "未运行".into(),
            headline: "Core 已停止，已清空上一运行代次的代理可用性状态".into(),
            target_url: self.config.target_url.clone(),
            config: self.config.clone(),
            suggestions: vec!["点击总览「启动 Core」后，后台将自动完成代理可用性探测。".into()],
            ..Default::default()
        };
    }

    pub(crate) fn trigger_manual(
        &mut self,
        override_url: Option<String>,
    ) -> Result<ProxyAvailability> {
        let target_url = match override_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(raw) => parse_probe_url(raw)?.canonical_url,
            None => self.config.target_url.clone(),
        };
        let (ctx, token) = {
            let mut inner = self.inner.lock().unwrap();
            inner.last_client_activity = Instant::now();
            inner.was_tray_throttled = false;
            let Some(ctx) = inner.context.clone() else {
                return Err(Error::new(
                    "CORE_NOT_RUNNING",
                    "请先启动 Core 再检测代理可用性",
                ));
            };
            if select_probe_inbound(&ctx.config, &ctx.generation.http_inbound_tag).is_none() {
                return Err(Error::new(
                    "PROBE_INBOUND_UNSUPPORTED",
                    "当前配置没有可用的回环 HTTP / Mixed / SOCKS 入站",
                ));
            }
            inner.in_flight_token = inner.in_flight_token.wrapping_add(1);
            let token = inner.in_flight_token;
            inner.in_flight_generation = Some(ctx.generation.id.clone());
            inner.next_due = None;
            inner.snapshot.checking = true;
            inner.snapshot.target_url = target_url.clone();
            inner.snapshot.badge_text = "检测中…".into();
            inner.snapshot.overview_hint.clear();
            (ctx, token)
        };
        if let Some(handle) = self.task.take() {
            handle.abort();
        }
        self.spawn_probe(ctx, target_url, true, token);
        Ok(self.snapshot())
    }

    pub(crate) fn tick(&mut self) {
        if !self.auto_runtime_enabled || !self.config.enabled {
            return;
        }
        if self.task.as_ref().is_some_and(|h| !h.is_finished()) {
            return;
        }
        let spawn_args = {
            let mut inner = self.inner.lock().unwrap();
            let Some(ctx) = inner.context.clone() else {
                return;
            };
            if select_probe_inbound(&ctx.config, &ctx.generation.http_inbound_tag).is_none() {
                inner.next_due = None;
                return;
            }
            if inner.snapshot.checking {
                return;
            }
            let Some(due) = inner.next_due else {
                return;
            };
            if Instant::now() < due {
                return;
            }
            if self.config.tray_throttle
                && inner.last_client_activity.elapsed() >= Duration::from_secs(TRAY_IDLE_AFTER_SECS)
            {
                inner.was_tray_throttled = true;
            }
            inner.in_flight_token = inner.in_flight_token.wrapping_add(1);
            let token = inner.in_flight_token;
            inner.in_flight_generation = Some(ctx.generation.id.clone());
            inner.next_due = None;
            inner.snapshot.checking = true;
            if inner.snapshot.state == ProxyAvailabilityState::Untested {
                inner.snapshot.badge_text = "检测中…".into();
            }
            (ctx, self.config.target_url.clone(), token)
        };
        self.spawn_probe(spawn_args.0, spawn_args.1, false, spawn_args.2);
    }

    fn spawn_probe(&mut self, ctx: ProbeContext, target_url: String, manual: bool, token: u64) {
        let inner = Arc::clone(&self.inner);
        let cfg = self.config.clone();
        let auto_enabled = self.auto_runtime_enabled;
        self.task = Some(tokio::spawn(async move {
            let outcome = run_probe_once(ctx, target_url, cfg.timeout_secs, manual).await;
            let mut guard = inner.lock().unwrap();
            if guard.in_flight_token != token
                || guard.in_flight_generation.as_deref() != Some(outcome.generation_id.as_str())
            {
                return;
            }
            guard.in_flight_generation = None;
            let Some(active_ctx) = guard.context.clone() else {
                return;
            };
            if active_ctx.generation.id != outcome.generation_id
                || active_ctx.runtime_instance_id != outcome.runtime_instance_id
            {
                return;
            }
            apply_outcome(&mut guard, &active_ctx, &cfg, auto_enabled, outcome);
        }));
    }
}

fn apply_outcome(
    guard: &mut ProbeInner,
    ctx: &ProbeContext,
    cfg: &ProxyProbeConfig,
    auto_runtime_enabled: bool,
    outcome: ProbeOutcome,
) {
    let prev_state = guard.snapshot.state;
    let prev_failures = guard.snapshot.consecutive_failures;
    let is_direct_route = outcome
        .route
        .as_ref()
        .and_then(|r| r.expected_outbound.as_deref())
        == Some("direct");
    let proxy_owned = ctx.proxy_state == "Owned"
        && matches!(
            ctx.generation.proxy_mode,
            ProxyMode::Global | ProxyMode::Pac
        );

    let (
        next_state,
        badge_text,
        overview_hint,
        headline,
        consecutive_failures,
        next_delay_secs,
        suggestions,
        sample_note,
    ) = if outcome.inbound.is_none() {
        (
            ProxyAvailabilityState::Unsupported,
            "无法检测".to_owned(),
            "当前配置无可用本地代理入口".to_owned(),
            "当前配置不包含回环 HTTP / Mixed / SOCKS 入站，无法发起代理实测".to_owned(),
            0,
            None,
            vec!["如需浏览器代理与可用性检测，请在「配置 → 更多与源码 → 本地入口」补充回环 HTTP 或 Mixed 入站。".into()],
            "无可用代理入站".to_owned(),
        )
    } else if let Some(code) = outcome.http_status {
        let total_ms = outcome.total_ms.unwrap_or(0);
        if is_direct_route {
            (
                ProxyAvailabilityState::DirectOnly,
                "直连可达（未走代理）".to_owned(),
                "当前检测地址未走代理，建议更换".to_owned(),
                format!(
                    "探测地址命中 direct 规则（HTTP {code} · {total_ms} ms），未经过代理节点"
                ),
                0,
                Some(cfg.interval_secs),
                vec![
                    "当前探测目标在路由表中匹配 direct，只能证明本地直连正常，不能代表海外节点可用。".into(),
                    "请在「设置 → 代理探测」更换为走代理出站的探测地址（如 https://cp.cloudflare.com/generate_204）。".into(),
                ],
                "命中 direct 未走代理".to_owned(),
            )
        } else if total_ms > SLOW_LATENCY_MS {
            (
                ProxyAvailabilityState::Degraded,
                format!("响应偏慢 · {total_ms} ms"),
                if proxy_owned {
                    String::new()
                } else {
                    "当前系统代理为直连，流量暂未接管".into()
                },
                format!("代理端到端连通，但响应偏慢（HTTP {code} · {total_ms} ms）"),
                0,
                Some(cfg.interval_secs),
                vec![
                    "代理通道可用但延迟偏高（> 1500 ms），可关注后续自动巡检趋势或切换更低延迟节点。".into(),
                ],
                format!("响应偏慢 ({total_ms} ms)"),
            )
        } else {
            (
                ProxyAvailabilityState::Available,
                format!("可用 · {total_ms} ms"),
                if proxy_owned {
                    String::new()
                } else {
                    "当前系统代理为直连，流量暂未接管".into()
                },
                if proxy_owned {
                    format!("Xray 代理链路畅通（HTTP {code} · {total_ms} ms）")
                } else {
                    format!(
                        "Xray 本地入口与代理节点均可用（HTTP {code} · {total_ms} ms；当前系统代理为直连）"
                    )
                },
                0,
                Some(cfg.interval_secs),
                if proxy_owned {
                    vec![
                        "端到端实测通过，当前代理规则与节点状态健康。".into(),
                        "如特定网站打不开，可在下方使用「按指定地址诊断」单独测试该网址。".into(),
                    ]
                } else {
                    vec![
                        "Xray 代理通道本身可用；若需系统应用自动走代理，请在「总览」切换为「PAC」或「全局」。".into(),
                    ]
                },
                if outcome.manual {
                    "手动检测通过".into()
                } else {
                    "自动巡检通过".into()
                },
            )
        }
    } else {
        // Failure handling with 3-second anti-flap retry on 1st transient auto failure
        let failures = prev_failures.saturating_add(1);
        let stage = outcome
            .fail_stage
            .as_deref()
            .unwrap_or("经由代理端到端探测失败");
        if !outcome.manual
            && failures == 1
            && matches!(prev_state, ProxyAvailabilityState::Available)
        {
            (
                ProxyAvailabilityState::Degraded,
                "网络波动 · 复测中".to_owned(),
                "单次超时，正在自动复测".to_owned(),
                format!("最近一次自动探测失败（{stage}），将在 3 秒后自动复测确认"),
                failures,
                Some(ANTIFLAP_RETRY_SECS),
                vec![
                    "防抖机制：单次偶发失败暂不标红，3 秒后将自动复测；若连续 2 次失败将切为「不可用」。".into(),
                ],
                "单次异常 (3s 防抖复测)".to_owned(),
            )
        } else {
            let overview_hint = if outcome.is_local_failure {
                "本地代理入口异常，请查看诊断详情".to_owned()
            } else {
                "远端节点无响应，请查看诊断详情".to_owned()
            };
            let retry_secs = if failures >= 5 {
                UNAVAILABLE_BACKOFF_SECS
            } else {
                UNAVAILABLE_FAST_RETRY_SECS
            };
            let mut sugs = Vec::new();
            if outcome.is_local_failure {
                sugs.push("本地代理入口连接或认证未通过，请核对「配置 → 更多与源码 → 本地入口」的端口与账号设置。".into());
            } else {
                sugs.push("本地入站正常但经由代理无法访问目标：重点核查节点地址/端口、认证凭据（UUID/密码）、Reality 公钥及本机系统时间。".into());
                sugs.push("请前往「日志」页筛选 Core 错误输出查看具体握手报错；若需临时恢复直连，可在总览将代理模式切回「直连」。".into());
            }
            (
                ProxyAvailabilityState::Unavailable,
                "不可用".to_owned(),
                overview_hint,
                format!("代理检测失败（连续 {failures} 次）：{stage}"),
                failures,
                Some(retry_secs),
                sugs,
                stage.to_owned(),
            )
        }
    };

    let sample = ProxyProbeSample {
        timestamp: short_timestamp_hms(&outcome.checked_at),
        state: next_state,
        total_ms: outcome.total_ms,
        tls_ms: outcome.tls_ms,
        http_status: outcome.http_status,
        note: sample_note,
    };
    if guard.history.len() >= MAX_HISTORY_SAMPLES {
        guard.history.pop_front();
    }
    guard.history.push_back(sample);

    let steps = build_steps(ctx, Some(&outcome), &outcome.target_url, false);
    let tray_throttled = auto_runtime_enabled
        && cfg.enabled
        && cfg.tray_throttle
        && guard.last_client_activity.elapsed() >= Duration::from_secs(TRAY_IDLE_AFTER_SECS);
    if tray_throttled {
        guard.was_tray_throttled = true;
    }

    guard.next_due = if auto_runtime_enabled && cfg.enabled {
        next_delay_secs.map(|base| {
            let effective = if tray_throttled && base > ANTIFLAP_RETRY_SECS {
                base.max(TRAY_THROTTLED_INTERVAL_SECS)
            } else {
                base
            };
            Instant::now() + Duration::from_secs(effective)
        })
    } else {
        None
    };

    guard.snapshot = ProxyAvailability {
        generation_id: Some(outcome.generation_id),
        runtime_instance_id: Some(outcome.runtime_instance_id),
        state: next_state,
        checking: false,
        badge_text,
        overview_hint,
        headline,
        target_url: outcome.target_url,
        checked_at: Some(outcome.checked_at),
        local_ms: outcome.local_ms,
        tls_ms: outcome.tls_ms,
        total_ms: outcome.total_ms,
        http_status: outcome.http_status,
        inbound_tag: outcome.inbound.as_ref().map(|i| i.tag.clone()),
        inbound_protocol: outcome.inbound.as_ref().map(|i| i.protocol.clone()),
        expected_outbound: outcome.route.and_then(|r| r.expected_outbound),
        fail_stage: outcome.fail_stage,
        fail_detail: outcome.fail_detail,
        consecutive_failures,
        suggestions,
        steps,
        history: guard.history.iter().cloned().collect(),
        observatory: outcome.observatory,
        config: cfg.clone(),
        tray_throttled,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use proxy_model::{Candidate, CoreIdentity, Resources};
    use serde_json::json;
    use tokio::net::TcpListener;

    fn sample_context(
        config: Value,
        proxy_mode: ProxyMode,
        pac_script: Option<String>,
    ) -> ProbeContext {
        ProbeContext {
            generation: Generation {
                id: "gen-probe-1".into(),
                transaction_id: "tx-1".into(),
                request_id: "req-1".into(),
                candidate: Candidate {
                    id: "cand-1".into(),
                    digest: "digest-1".into(),
                    profile_id: "prof-1".into(),
                    base_revision: "rev-1".into(),
                    revision: "rev-1".into(),
                    config_path: PathBuf::from("config.json"),
                    core: CoreIdentity {
                        path: PathBuf::from("xray"),
                        sha256: "abc".into(),
                        version: "Xray 1.8".into(),
                    },
                    resources: Resources {
                        id: "res-1".into(),
                        root: PathBuf::from("res"),
                        files: vec![],
                        complete: true,
                        notes: vec![],
                        dynamic: vec![],
                    },
                },
                proxy_mode,
                http_inbound_tag: "http-in".into(),
                running: true,
                pac: None,
            },
            runtime_instance_id: "inst-1".into(),
            profile_display_name: "测试配置".into(),
            proxy_state: if proxy_mode == ProxyMode::Direct {
                "Released".into()
            } else {
                "Owned".into()
            },
            config,
            geodata: GeoData::default(),
            pac_script,
        }
    }

    #[test]
    fn parse_probe_url_validates_http_https_and_rejects_bad_inputs() {
        let u1 = parse_probe_url("https://cp.cloudflare.com/generate_204").unwrap();
        assert!(u1.scheme_https);
        assert_eq!(u1.host, "cp.cloudflare.com");
        assert_eq!(u1.port, 443);
        assert_eq!(u1.path_and_query, "/generate_204");

        let u2 = parse_probe_url("http://[::1]:8080/health?v=1#frag").unwrap();
        assert!(!u2.scheme_https);
        assert_eq!(u2.host, "::1");
        assert_eq!(u2.port, 8080);
        assert_eq!(u2.canonical_url, "http://[::1]:8080/health?v=1");

        assert!(parse_probe_url("ftp://example.com").is_err());
        assert!(parse_probe_url("http://user:pass@example.com").is_err());
        assert!(parse_probe_url("https://bad host/a").is_err());
    }

    #[test]
    fn probe_config_validation_and_persistence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProxyProbeManager::load(dir.path(), false).unwrap();
        assert_eq!(mgr.snapshot().config.target_url, DEFAULT_PROXY_PROBE_URL);

        let err = mgr.save_config(ProxyProbeConfig {
            target_url: "socks5://bad".into(),
            ..Default::default()
        });
        assert!(err.is_err());

        let saved = mgr
            .save_config(ProxyProbeConfig {
                enabled: false,
                target_url: "http://cp.cloudflare.com/generate_204".into(),
                interval_secs: 5, // clamps to 10
                timeout_secs: 99, // clamps to 30
                tray_throttle: false,
            })
            .unwrap();
        assert!(!saved.config.enabled);
        assert_eq!(saved.config.interval_secs, 10);
        assert_eq!(saved.config.timeout_secs, 30);

        let reloaded = ProxyProbeManager::load(dir.path(), false).unwrap();
        assert_eq!(reloaded.snapshot().config, saved.config);
    }

    #[tokio::test]
    async fn http_probe_through_mock_http_proxy_reports_available_and_five_steps() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 512];
            let n = sock.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            assert!(req.starts_with("GET http://cp.cloudflare.com/generate_204 HTTP/1.1"));
            assert!(req.contains("Proxy-Authorization: Basic dTE6cDE="));
            sock.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });

        let cfg = json!({
            "inbounds": [{
                "tag": "http-in",
                "listen": "127.0.0.1",
                "port": addr.port(),
                "protocol": "http",
                "settings": {"accounts": [{"user": "u1", "pass": "p1"}]}
            }],
            "outbounds": [{"tag": "proxy-hk", "protocol": "vless"}],
            "routing": {"rules": [{"type": "field", "domain": ["example.cn"], "outboundTag": "direct"}]}
        });
        let ctx = sample_context(cfg, ProxyMode::Global, None);
        let outcome = run_probe_once(
            ctx.clone(),
            "http://cp.cloudflare.com/generate_204".into(),
            5,
            true,
        )
        .await;
        let mut inner = ProbeInner {
            snapshot: ProxyAvailability::default(),
            history: VecDeque::new(),
            context: Some(ctx.clone()),
            next_due: None,
            last_client_activity: Instant::now(),
            was_tray_throttled: false,
            in_flight_token: 1,
            in_flight_generation: None,
        };
        apply_outcome(
            &mut inner,
            &ctx,
            &ProxyProbeConfig::default(),
            true,
            outcome,
        );
        let snap = &inner.snapshot;
        assert_eq!(snap.state, ProxyAvailabilityState::Available);
        assert!(snap.overview_hint.is_empty());
        assert_eq!(snap.http_status, Some(204));
        assert_eq!(snap.steps.len(), 5);
        assert!(
            snap.steps
                .iter()
                .all(|s| s.status == DiagnosticStepStatus::Pass)
        );
        assert_eq!(snap.history.len(), 1);
    }

    #[tokio::test]
    async fn socks5_probe_through_mock_socks5_proxy_with_auth_succeeds() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut greet = [0u8; 3];
            sock.read_exact(&mut greet).await.unwrap();
            assert_eq!(greet, [5, 1, 2]);
            sock.write_all(&[5, 2]).await.unwrap();

            let mut auth = [0u8; 64];
            let n = sock.read(&mut auth).await.unwrap();
            assert_eq!(auth[0], 1);
            assert!(n >= 5);
            sock.write_all(&[1, 0]).await.unwrap();

            let mut req = [0u8; 128];
            let n = sock.read(&mut req).await.unwrap();
            assert_eq!(&req[..4], &[5, 1, 0, 3]);
            assert!(n > 6);
            sock.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
                .await
                .unwrap();

            let mut http_buf = [0u8; 512];
            let n = sock.read(&mut http_buf).await.unwrap();
            assert!(
                String::from_utf8_lossy(&http_buf[..n]).starts_with("GET /generate_204 HTTP/1.1")
            );
            sock.write_all(b"HTTP/1.1 204 No Content\r\n\r\n")
                .await
                .unwrap();
        });

        let cfg = json!({
            "inbounds": [{
                "tag": "socks-in",
                "listen": "127.0.0.1",
                "port": addr.port(),
                "protocol": "socks",
                "settings": {"auth": "password", "accounts": [{"user": "alice", "pass": "secret"}]}
            }],
            "outbounds": [{"tag": "proxy", "protocol": "vless"}]
        });
        let ctx = sample_context(cfg, ProxyMode::Direct, None);
        let outcome =
            run_probe_once(ctx, "http://cp.cloudflare.com/generate_204".into(), 5, true).await;
        assert_eq!(outcome.http_status, Some(204));
        assert!(outcome.fail_stage.is_none());
    }

    #[tokio::test]
    async fn https_connect_200_followed_by_closed_upstream_fails_instead_of_false_positive() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 512];
            let _ = sock.read(&mut buf).await.unwrap();
            // Mimic Xray HTTP inbound returning CONNECT 200 before upstream fails.
            sock.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await
                .unwrap();
            let mut client_hello = [0u8; 512];
            let _ = sock.read(&mut client_hello).await;
            drop(sock);
        });

        let cfg = json!({
            "inbounds": [{
                "tag": "http-in",
                "listen": "127.0.0.1",
                "port": addr.port(),
                "protocol": "http"
            }],
            "outbounds": [{"tag": "proxy", "protocol": "vless"}]
        });
        let ctx = sample_context(cfg, ProxyMode::Global, None);
        let outcome = run_probe_once(
            ctx,
            "https://cp.cloudflare.com/generate_204".into(),
            5,
            true,
        )
        .await;
        assert_eq!(outcome.http_status, None);
        assert!(!outcome.is_local_failure);
        assert!(outcome.local_ms.is_some());
        assert!(
            outcome
                .fail_stage
                .as_deref()
                .is_some_and(|s| s.contains("TLS"))
        );
    }

    #[tokio::test]
    async fn direct_routed_target_and_pac_step_five_are_accurately_reported() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 512];
            let _ = sock.read(&mut buf).await.unwrap();
            sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });

        let cfg = json!({
            "inbounds": [{
                "tag": "http-in",
                "listen": "127.0.0.1",
                "port": addr.port(),
                "protocol": "http"
            }],
            "outbounds": [{"tag": "proxy", "protocol": "vless"}, {"tag": "direct", "protocol": "freedom"}],
            "routing": {"rules": [{"type": "field", "domain": ["baidu.com"], "outboundTag": "direct"}]}
        });
        let pac_script = "function FindProxyForURL(url, host) { if (host === 'www.baidu.com') return 'DIRECT'; return 'PROXY 127.0.0.1:10809'; }";
        let ctx = sample_context(cfg, ProxyMode::Pac, Some(pac_script.into()));
        let outcome = run_probe_once(ctx.clone(), "http://www.baidu.com/".into(), 5, true).await;
        assert_eq!(outcome.pac_decision, Some((true, "DIRECT".into())));
        let mut inner = ProbeInner {
            snapshot: ProxyAvailability::default(),
            history: VecDeque::new(),
            context: Some(ctx.clone()),
            next_due: None,
            last_client_activity: Instant::now(),
            was_tray_throttled: false,
            in_flight_token: 1,
            in_flight_generation: None,
        };
        apply_outcome(
            &mut inner,
            &ctx,
            &ProxyProbeConfig::default(),
            true,
            outcome,
        );
        assert_eq!(inner.snapshot.state, ProxyAvailabilityState::DirectOnly);
        assert_eq!(inner.snapshot.badge_text, "直连可达（未走代理）");
        assert_eq!(
            inner.snapshot.overview_hint,
            "当前检测地址未走代理，建议更换"
        );
        assert_eq!(inner.snapshot.steps[2].status, DiagnosticStepStatus::Warn);
        assert_eq!(inner.snapshot.steps[4].evidence.as_deref(), Some("DIRECT"));
    }

    #[test]
    fn antiflap_degrades_on_first_auto_failure_then_unavailable_on_second() {
        let cfg = json!({"inbounds": [{"tag": "http-in", "listen": "127.0.0.1", "port": 10809, "protocol": "http"}]});
        let ctx = sample_context(cfg, ProxyMode::Global, None);
        let probe_cfg = ProxyProbeConfig::default();
        let mut inner = ProbeInner {
            snapshot: ProxyAvailability {
                state: ProxyAvailabilityState::Available,
                ..Default::default()
            },
            history: VecDeque::new(),
            context: Some(ctx.clone()),
            next_due: None,
            last_client_activity: Instant::now(),
            was_tray_throttled: false,
            in_flight_token: 1,
            in_flight_generation: None,
        };
        let fail_outcome = |manual| ProbeOutcome {
            generation_id: ctx.generation.id.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            target_url: DEFAULT_PROXY_PROBE_URL.into(),
            checked_at: "2026-11-23 15:00:00.000 +08:00".into(),
            manual,
            inbound: select_probe_inbound(&ctx.config, "http-in"),
            route: None,
            pac_decision: None,
            local_ms: Some(1),
            tls_ms: None,
            total_ms: Some(5000),
            http_status: None,
            is_local_failure: false,
            fail_stage: Some("经由代理端到端探测超时".into()),
            fail_detail: Some("timed out".into()),
            observatory: vec![],
        };

        apply_outcome(&mut inner, &ctx, &probe_cfg, true, fail_outcome(false));
        assert_eq!(inner.snapshot.state, ProxyAvailabilityState::Degraded);
        assert_eq!(inner.snapshot.badge_text, "网络波动 · 复测中");
        assert_eq!(inner.snapshot.overview_hint, "单次超时，正在自动复测");

        apply_outcome(&mut inner, &ctx, &probe_cfg, true, fail_outcome(false));
        assert_eq!(inner.snapshot.state, ProxyAvailabilityState::Unavailable);
        assert_eq!(inner.snapshot.badge_text, "不可用");
        assert_eq!(inner.snapshot.consecutive_failures, 2);
    }

    #[test]
    fn parse_observatory_vars_reads_native_xray_expvars_json() {
        let raw = r#"{
            "observatory": {
                "proxy-b": {"alive": false, "delay": 0, "last_error_reason": "timeout"},
                "proxy-a": {"alive": true, "delay": 142, "outbound_tag": "proxy-a"}
            }
        }"#;
        let list = parse_observatory_vars(raw);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].outbound_tag, "proxy-a");
        assert!(list[0].alive);
        assert_eq!(list[0].delay_ms, Some(142));
        assert_eq!(list[1].outbound_tag, "proxy-b");
        assert!(!list[1].alive);
        assert_eq!(list[1].last_error.as_deref(), Some("timeout"));
    }
}
