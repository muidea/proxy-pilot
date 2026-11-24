//! Offline parser for standard proxy share URIs (`vless://`, `trojan://`, `ss://`, `vmess://`, `socks://`, `http://`).
//! Never performs network I/O and never fabricates missing remote credentials or addresses.

use base64::Engine as _;
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use url::Url;

const USERINFO_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'|')
    .add(b'%')
    .add(b'&')
    .add(b'+');

const QUERY_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'&')
    .add(b'+')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'%');

const FRAGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'%');

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedShareNode {
    pub index: usize,
    pub tag: String,
    pub name: String,
    pub protocol: String,
    pub uri: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EncodedShareBatch {
    pub nodes: Vec<EncodedShareNode>,
    pub skipped: Vec<(String, String)>,
    pub errors: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedShareNode {
    pub protocol: String,
    pub name: Option<String>,
    pub outbound: Value,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareLinkBatchError {
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShareLinkBatch {
    pub nodes: Vec<ParsedShareNode>,
    pub errors: Vec<ShareLinkBatchError>,
}

/// Parses one or more share links (newline-separated, or offline Base64-encoded lines).
pub fn parse_share_links(input: &str) -> ShareLinkBatch {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ShareLinkBatch::default();
    }

    let expanded;
    let source = if !trimmed.contains("://")
        && let Ok(bytes) = decode_base64_flex(trimmed)
        && let Ok(text) = String::from_utf8(bytes)
        && text.contains("://")
    {
        expanded = text;
        expanded.as_str()
    } else {
        trimmed
    };

    let mut batch = ShareLinkBatch::default();
    for (idx, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        match parse_share_link(line) {
            Ok(node) => batch.nodes.push(node),
            Err(message) => batch.errors.push(ShareLinkBatchError {
                line: idx + 1,
                message,
            }),
        }
    }
    batch
}

/// Parses a single proxy share URI into an Xray outbound JSON object (without assigning `tag`).
pub fn parse_share_link(raw: &str) -> std::result::Result<ParsedShareNode, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("链接内容为空".into());
    }
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return Err(
            "缺少协议前缀（支持 vless://、trojan://、ss://、vmess://、socks://、http://）".into(),
        );
    };
    match scheme.to_ascii_lowercase().as_str() {
        "vless" => parse_vless(trimmed),
        "trojan" => parse_trojan(trimmed),
        "ss" => parse_ss(trimmed, rest),
        "vmess" => parse_vmess(rest),
        "socks" | "socks5" => parse_upstream(trimmed, "socks"),
        "http" | "https" => parse_upstream(trimmed, "http"),
        other => Err(format!("暂不支持的分享链接协议：{other}://")),
    }
}

fn decode_base64_flex(input: &str) -> std::result::Result<Vec<u8>, String> {
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    for engine in [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ] {
        if let Ok(bytes) = engine.decode(&compact) {
            return Ok(bytes);
        }
    }
    Err("Base64 内容无法解码".into())
}

fn decode_component(raw: &str) -> String {
    percent_decode_str(raw).decode_utf8_lossy().into_owned()
}

fn clean_host(url: &Url) -> std::result::Result<String, String> {
    let host = url
        .host_str()
        .map(|h| h.trim_start_matches('[').trim_end_matches(']'))
        .filter(|h| !h.is_empty())
        .ok_or_else(|| "链接缺少服务器地址".to_owned())?;
    Ok(host.to_owned())
}

fn required_port(url: &Url, default_port: Option<u16>) -> std::result::Result<u16, String> {
    url.port()
        .or(default_port)
        .filter(|p| *p > 0)
        .ok_or_else(|| "链接缺少有效端口（1–65535）".to_owned())
}

fn fragment_name(url: &Url) -> Option<String> {
    url.fragment()
        .map(decode_component)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn collect_query(url: &Url) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (k, v) in url.query_pairs() {
        map.insert(k.into_owned(), v.into_owned());
    }
    map
}

fn is_truthy(v: &str) -> bool {
    matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes")
}

fn build_stream_settings(
    params: &BTreeMap<String, String>,
    default_security: &str,
    warnings: &mut Vec<String>,
) -> Value {
    let network = params
        .get("type")
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "tcp".into());
    let security = params
        .get("security")
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_security.into());

    let mut stream = json!({
        "network": network,
        "security": security,
    });

    match security.as_str() {
        "tls" => {
            let mut tls = serde_json::Map::new();
            if let Some(sni) = params.get("sni").filter(|s| !s.trim().is_empty()) {
                tls.insert("serverName".into(), json!(sni.trim()));
            }
            if let Some(fp) = params.get("fp").filter(|s| !s.trim().is_empty()) {
                tls.insert("fingerprint".into(), json!(fp.trim()));
            }
            if let Some(alpn) = params.get("alpn").filter(|s| !s.trim().is_empty()) {
                let list: Vec<_> = alpn
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                if !list.is_empty() {
                    tls.insert("alpn".into(), json!(list));
                }
            }
            if params
                .get("allowInsecure")
                .or_else(|| params.get("insecure"))
                .is_some_and(|v| is_truthy(v))
            {
                tls.insert("allowInsecure".into(), json!(true));
                warnings.push(
                    "链接包含 allowInsecure=1，已关闭 TLS 证书验证；导入后可在节点卡片中恢复验证。"
                        .into(),
                );
            }
            stream["tlsSettings"] = Value::Object(tls);
        }
        "reality" => {
            let mut reality = serde_json::Map::new();
            if let Some(sni) = params.get("sni").filter(|s| !s.trim().is_empty()) {
                reality.insert("serverName".into(), json!(sni.trim()));
            }
            if let Some(fp) = params.get("fp").filter(|s| !s.trim().is_empty()) {
                reality.insert("fingerprint".into(), json!(fp.trim()));
            }
            if let Some(pbk) = params.get("pbk").filter(|s| !s.trim().is_empty()) {
                reality.insert("publicKey".into(), json!(pbk.trim()));
            }
            if let Some(sid) = params.get("sid").filter(|s| !s.trim().is_empty()) {
                reality.insert("shortId".into(), json!(sid.trim()));
            }
            if let Some(spx) = params.get("spx").filter(|s| !s.trim().is_empty()) {
                reality.insert("spiderX".into(), json!(spx.trim()));
            }
            if let Some(pqv) = params.get("pqv").filter(|s| !s.trim().is_empty()) {
                reality.insert("mldsa65Verify".into(), json!(pqv.trim()));
            }
            stream["realitySettings"] = Value::Object(reality);
        }
        "none" => {}
        other => {
            warnings.push(format!("未识别的安全类型 security={other}，已原样保留。"));
        }
    }

    match network.as_str() {
        "ws" => {
            let mut ws = serde_json::Map::new();
            if let Some(path) = params.get("path").filter(|s| !s.is_empty()) {
                ws.insert("path".into(), json!(path));
            }
            if let Some(host) = params.get("host").filter(|s| !s.trim().is_empty()) {
                ws.insert("headers".into(), json!({ "Host": host.trim() }));
            }
            if !ws.is_empty() {
                stream["wsSettings"] = Value::Object(ws);
            }
        }
        "grpc" => {
            let mut grpc = serde_json::Map::new();
            if let Some(service) = params
                .get("serviceName")
                .or_else(|| params.get("path"))
                .filter(|s| !s.trim().is_empty())
            {
                grpc.insert("serviceName".into(), json!(service.trim()));
            }
            if params
                .get("mode")
                .is_some_and(|m| m.eq_ignore_ascii_case("multi"))
            {
                grpc.insert("multiMode".into(), json!(true));
            }
            if !grpc.is_empty() {
                stream["grpcSettings"] = Value::Object(grpc);
            }
        }
        "httpupgrade" => {
            let mut hu = serde_json::Map::new();
            if let Some(path) = params.get("path").filter(|s| !s.is_empty()) {
                hu.insert("path".into(), json!(path));
            }
            if let Some(host) = params.get("host").filter(|s| !s.trim().is_empty()) {
                hu.insert("host".into(), json!(host.trim()));
            }
            if !hu.is_empty() {
                stream["httpupgradeSettings"] = Value::Object(hu);
            }
        }
        "splithttp" | "xhttp" => {
            let mut xh = serde_json::Map::new();
            if let Some(path) = params.get("path").filter(|s| !s.is_empty()) {
                xh.insert("path".into(), json!(path));
            }
            if let Some(host) = params.get("host").filter(|s| !s.trim().is_empty()) {
                xh.insert("host".into(), json!(host.trim()));
            }
            if let Some(mode) = params.get("mode").filter(|s| !s.trim().is_empty()) {
                xh.insert("mode".into(), json!(mode.trim()));
            }
            if !xh.is_empty() {
                let key = if network == "splithttp" {
                    "splithttpSettings"
                } else {
                    "xhttpSettings"
                };
                stream[key] = Value::Object(xh);
            }
        }
        "kcp" => {
            let mut kcp = serde_json::Map::new();
            if let Some(seed) = params.get("seed").filter(|s| !s.is_empty()) {
                kcp.insert("seed".into(), json!(seed));
            }
            if let Some(header) = params
                .get("headerType")
                .filter(|s| !s.trim().is_empty() && *s != "none")
            {
                kcp.insert("header".into(), json!({ "type": header.trim() }));
            }
            if !kcp.is_empty() {
                stream["kcpSettings"] = Value::Object(kcp);
            }
        }
        "quic" => {
            let mut quic = serde_json::Map::new();
            if let Some(sec) = params.get("quicSecurity").filter(|s| !s.trim().is_empty()) {
                quic.insert("security".into(), json!(sec.trim()));
            }
            if let Some(key) = params.get("key").filter(|s| !s.is_empty()) {
                quic.insert("key".into(), json!(key));
            }
            if let Some(header) = params
                .get("headerType")
                .filter(|s| !s.trim().is_empty() && *s != "none")
            {
                quic.insert("header".into(), json!({ "type": header.trim() }));
            }
            if !quic.is_empty() {
                stream["quicSettings"] = Value::Object(quic);
            }
        }
        "tcp" | "raw" => {
            if params
                .get("headerType")
                .is_some_and(|h| h.eq_ignore_ascii_case("http"))
            {
                let mut req = serde_json::Map::new();
                if let Some(path) = params.get("path").filter(|s| !s.is_empty()) {
                    let paths: Vec<_> = path
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !paths.is_empty() {
                        req.insert("path".into(), json!(paths));
                    }
                }
                if let Some(host) = params.get("host").filter(|s| !s.trim().is_empty()) {
                    let hosts: Vec<_> = host
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !hosts.is_empty() {
                        req.insert("headers".into(), json!({ "Host": hosts }));
                    }
                }
                let mut header = json!({ "type": "http" });
                if !req.is_empty() {
                    header["request"] = Value::Object(req);
                }
                let key = if network == "raw" {
                    "rawSettings"
                } else {
                    "tcpSettings"
                };
                stream[key] = json!({ "header": header });
            }
        }
        other => {
            warnings.push(format!(
                "传输方式 {other} 的专有参数未自动映射，请在节点表单或高级编辑中核对。"
            ));
        }
    }

    stream
}

fn parse_vless(raw: &str) -> std::result::Result<ParsedShareNode, String> {
    let url = Url::parse(raw).map_err(|e| format!("VLESS 链接格式无效：{e}"))?;
    let address = clean_host(&url)?;
    let port = required_port(&url, None)?;
    let uuid = decode_component(url.username());
    if uuid.trim().is_empty() {
        return Err("VLESS 链接缺少用户 ID / UUID".into());
    }
    let params = collect_query(&url);
    let mut warnings = Vec::new();
    let encryption = params
        .get("encryption")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("none");
    let mut user = json!({
        "id": uuid,
        "encryption": encryption,
    });
    if let Some(flow) = params.get("flow").filter(|s| !s.trim().is_empty()) {
        user["flow"] = json!(flow.trim());
    }
    let stream = build_stream_settings(&params, "none", &mut warnings);
    let outbound = json!({
        "protocol": "vless",
        "settings": {
            "vnext": [{
                "address": address,
                "port": port,
                "users": [user],
            }]
        },
        "streamSettings": stream,
    });
    Ok(ParsedShareNode {
        protocol: "vless".into(),
        name: fragment_name(&url),
        outbound,
        warnings,
    })
}

fn parse_trojan(raw: &str) -> std::result::Result<ParsedShareNode, String> {
    let url = Url::parse(raw).map_err(|e| format!("Trojan 链接格式无效：{e}"))?;
    let address = clean_host(&url)?;
    let port = required_port(&url, Some(443))?;
    let password = decode_component(url.username());
    if password.is_empty() {
        return Err("Trojan 链接缺少密码".into());
    }
    let params = collect_query(&url);
    let mut warnings = Vec::new();
    let stream = build_stream_settings(&params, "tls", &mut warnings);
    let outbound = json!({
        "protocol": "trojan",
        "settings": {
            "servers": [{
                "address": address,
                "port": port,
                "password": password,
            }]
        },
        "streamSettings": stream,
    });
    Ok(ParsedShareNode {
        protocol: "trojan".into(),
        name: fragment_name(&url),
        outbound,
        warnings,
    })
}

fn parse_ss(raw: &str, rest: &str) -> std::result::Result<ParsedShareNode, String> {
    // Support both SIP002 (`ss://userinfo@host:port#name`) and legacy (`ss://base64(method:pass@host:port)#name`).
    let (body_no_frag, frag_raw) = match rest.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (rest, None),
    };
    let name = frag_raw
        .map(decode_component)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let mut warnings = Vec::new();
    if let Ok(url) = Url::parse(raw)
        && url.host_str().is_some()
        && !url.username().is_empty()
    {
        let address = clean_host(&url)?;
        let port = required_port(&url, None)?;
        let userinfo = decode_component(url.username());
        let (method, password) = if let Some((m, p)) = userinfo.split_once(':') {
            (m.to_owned(), p.to_owned())
        } else {
            let decoded = String::from_utf8(
                decode_base64_flex(&userinfo).map_err(|_| "Shadowsocks 认证信息编码无效")?,
            )
            .map_err(|_| "Shadowsocks 认证信息不是有效 UTF-8")?;
            let (m, p) = decoded
                .split_once(':')
                .ok_or_else(|| "Shadowsocks 认证信息缺少 method:password 分隔符".to_owned())?;
            (m.to_owned(), p.to_owned())
        };
        if method.trim().is_empty() || password.is_empty() {
            return Err("Shadowsocks 链接缺少加密方式或密码".into());
        }
        let params = collect_query(&url);
        if let Some(plugin) = params.get("plugin").filter(|s| !s.trim().is_empty()) {
            warnings.push(format!(
                "链接附带 SIP003 plugin 参数（{plugin}），Xray 原生不自动加载外部插件，请按需在高级字段核对。"
            ));
        }
        let outbound = json!({
            "protocol": "shadowsocks",
            "settings": {
                "servers": [{
                    "address": address,
                    "port": port,
                    "method": method,
                    "password": password,
                }]
            }
        });
        return Ok(ParsedShareNode {
            protocol: "shadowsocks".into(),
            name: name.or_else(|| fragment_name(&url)),
            outbound,
            warnings,
        });
    }

    // Legacy `ss://base64(method:password@host:port)` (ignore optional trailing `/?...` before `#`)
    let encoded = body_no_frag
        .split_once('/')
        .map_or(body_no_frag, |(a, _)| a);
    let decoded =
        String::from_utf8(decode_base64_flex(encoded).map_err(|_| "Shadowsocks 链接无法解析")?)
            .map_err(|_| "Shadowsocks 解码结果不是有效 UTF-8")?;
    let (creds, host_port) = decoded
        .rsplit_once('@')
        .ok_or_else(|| "Shadowsocks 链接缺少 @ 服务器信息".to_owned())?;
    let (method, password) = creds
        .split_once(':')
        .ok_or_else(|| "Shadowsocks 缺少 method:password".to_owned())?;
    let synthetic = Url::parse(&format!("ss://user@{host_port}"))
        .map_err(|_| "Shadowsocks 服务器地址或端口格式无效")?;
    let address = clean_host(&synthetic)?;
    let port = required_port(&synthetic, None)?;
    if method.trim().is_empty() || password.is_empty() {
        return Err("Shadowsocks 链接缺少加密方式或密码".into());
    }
    let outbound = json!({
        "protocol": "shadowsocks",
        "settings": {
            "servers": [{
                "address": address,
                "port": port,
                "method": method,
                "password": password,
            }]
        }
    });
    Ok(ParsedShareNode {
        protocol: "shadowsocks".into(),
        name,
        outbound,
        warnings,
    })
}

fn json_string(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn parse_vmess(rest: &str) -> std::result::Result<ParsedShareNode, String> {
    let (payload, frag_name) = match rest.split_once('#') {
        Some((p, f)) => {
            let decoded = decode_component(f).trim().to_owned();
            (p, (!decoded.is_empty()).then_some(decoded))
        }
        None => (rest, None),
    };
    let bytes = decode_base64_flex(payload).map_err(|_| "VMess Base64 内容无效")?;
    let v: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("VMess JSON 解析失败：{e}"))?;
    let address = json_string(&v, "add");
    if address.is_empty() {
        return Err("VMess 配置缺少服务器地址 (add)".into());
    }
    let port: u16 = json_string(&v, "port")
        .parse()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| "VMess 配置缺少有效端口 (port)".to_owned())?;
    let id = json_string(&v, "id");
    if id.is_empty() {
        return Err("VMess 配置缺少用户 ID (id)".into());
    }
    let security_cipher = {
        let scy = json_string(&v, "scy");
        if scy.is_empty() { "auto".into() } else { scy }
    };
    let mut user = json!({
        "id": id,
        "security": security_cipher,
    });
    if let Ok(aid) = json_string(&v, "aid").parse::<u64>()
        && aid > 0
    {
        user["alterId"] = json!(aid);
    }

    let mut params = BTreeMap::new();
    let net = json_string(&v, "net");
    if !net.is_empty() {
        params.insert("type".into(), net);
    }
    let tls = json_string(&v, "tls");
    if !tls.is_empty() {
        params.insert("security".into(), tls);
    }
    for (src, dst) in [
        ("sni", "sni"),
        ("fp", "fp"),
        ("alpn", "alpn"),
        ("host", "host"),
        ("path", "path"),
        ("type", "headerType"),
    ] {
        let val = json_string(&v, src);
        if !val.is_empty() {
            params.insert(dst.into(), val);
        }
    }
    if v.get("allowInsecure").is_some_and(|val| {
        val.as_bool() == Some(true)
            || val.as_u64() == Some(1)
            || val.as_str().is_some_and(is_truthy)
    }) {
        params.insert("allowInsecure".into(), "1".into());
    }
    if params.get("type").is_some_and(|t| t == "grpc")
        && let Some(path) = params.get("path").cloned()
    {
        params.insert("serviceName".into(), path);
    }

    let mut warnings = Vec::new();
    let stream = build_stream_settings(&params, "none", &mut warnings);
    let ps = json_string(&v, "ps");
    let name = if ps.is_empty() { frag_name } else { Some(ps) };
    let outbound = json!({
        "protocol": "vmess",
        "settings": {
            "vnext": [{
                "address": address,
                "port": port,
                "users": [user],
            }]
        },
        "streamSettings": stream,
    });
    Ok(ParsedShareNode {
        protocol: "vmess".into(),
        name,
        outbound,
        warnings,
    })
}

fn parse_upstream(raw: &str, protocol: &str) -> std::result::Result<ParsedShareNode, String> {
    let url = Url::parse(raw).map_err(|e| format!("{protocol} 链接格式无效：{e}"))?;
    let address = clean_host(&url)?;
    let default_port = if url.scheme().eq_ignore_ascii_case("https") {
        Some(443)
    } else {
        None
    };
    let port = required_port(&url, default_port)?;
    let mut server = json!({
        "address": address,
        "port": port,
    });
    let user = decode_component(url.username());
    let pass = url.password().map(decode_component).unwrap_or_default();
    if !user.is_empty() {
        server["users"] = json!([{
            "user": user,
            "pass": pass,
        }]);
    } else if url.password().is_some() {
        return Err("认证信息缺少用户名".into());
    }
    let mut warnings = Vec::new();
    let params = collect_query(&url);
    let default_sec = if url.scheme().eq_ignore_ascii_case("https") {
        "tls"
    } else {
        "none"
    };
    let mut outbound = json!({
        "protocol": protocol,
        "settings": {
            "servers": [server]
        }
    });
    if url.scheme().eq_ignore_ascii_case("https")
        || params.contains_key("security")
        || params.contains_key("type")
    {
        outbound["streamSettings"] = build_stream_settings(&params, default_sec, &mut warnings);
    }
    Ok(ParsedShareNode {
        protocol: protocol.into(),
        name: fragment_name(&url),
        outbound,
        warnings,
    })
}

/// Returns true if an Xray outbound protocol can be represented as a standard share URI.
pub fn is_shareable_protocol(protocol: &str) -> bool {
    matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "vless" | "trojan" | "shadowsocks" | "vmess" | "socks" | "http"
    )
}

fn format_authority_host(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed.contains(':') && !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        format!("[{trimmed}]")
    } else {
        trimmed.to_owned()
    }
}

fn encode_userinfo(raw: &str) -> String {
    utf8_percent_encode(raw, USERINFO_ENCODE_SET).to_string()
}

fn encode_query_pairs(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .map(|(k, v)| {
            format!(
                "{}={}",
                utf8_percent_encode(k, QUERY_ENCODE_SET),
                utf8_percent_encode(v, QUERY_ENCODE_SET)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn encode_fragment(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("#{}", utf8_percent_encode(trimmed, FRAGMENT_ENCODE_SET))
    }
}

fn non_empty_str(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn required_outbound_port(value: Option<&Value>) -> std::result::Result<u16, String> {
    let port = match value {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => s.trim().parse::<u64>().ok(),
        _ => None,
    };
    port.and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0)
        .ok_or_else(|| "节点缺少有效端口（1–65535）".to_owned())
}

fn collect_lossy_outbound_warnings(outbound: &Value, warnings: &mut Vec<String>) {
    if let Some(obj) = outbound.as_object() {
        for key in obj.keys() {
            if !matches!(
                key.as_str(),
                "tag" | "protocol" | "settings" | "streamSettings"
            ) {
                warnings.push(format!(
                    "出站包含高级字段 `{key}`，标准分享链接无法表达，已省略。"
                ));
            }
        }
    }
    if let Some(vnext) = outbound
        .pointer("/settings/vnext")
        .and_then(Value::as_array)
    {
        if vnext.len() > 1 {
            warnings.push("存在多个服务器条目（vnext），分享链接仅导出第一项。".into());
        }
        if vnext
            .first()
            .and_then(|v| v.get("users"))
            .and_then(Value::as_array)
            .is_some_and(|u| u.len() > 1)
        {
            warnings.push("存在多个用户条目（users），分享链接仅导出第一项。".into());
        }
    }
    if let Some(servers) = outbound
        .pointer("/settings/servers")
        .and_then(Value::as_array)
    {
        if servers.len() > 1 {
            warnings.push("存在多个服务器条目（servers），分享链接仅导出第一项。".into());
        }
        if servers
            .first()
            .and_then(|s| s.get("users"))
            .and_then(Value::as_array)
            .is_some_and(|u| u.len() > 1)
        {
            warnings.push("存在多个用户条目（users），分享链接仅导出第一项。".into());
        }
    }
    if let Some(stream) = outbound.get("streamSettings").and_then(Value::as_object) {
        for key in stream.keys() {
            if !matches!(
                key.as_str(),
                "network"
                    | "security"
                    | "tlsSettings"
                    | "realitySettings"
                    | "wsSettings"
                    | "grpcSettings"
                    | "httpupgradeSettings"
                    | "splithttpSettings"
                    | "xhttpSettings"
                    | "kcpSettings"
                    | "quicSettings"
                    | "tcpSettings"
                    | "rawSettings"
            ) {
                warnings.push(format!(
                    "传输配置包含高级字段 `streamSettings.{key}`，标准分享链接无法表达，已省略。"
                ));
            }
        }
        if let Some(headers) = stream
            .get("wsSettings")
            .and_then(|w| w.get("headers"))
            .and_then(Value::as_object)
            && headers.keys().any(|k| !k.eq_ignore_ascii_case("Host"))
        {
            warnings.push("WebSocket 自定义请求头（除 Host 外）未包含在分享链接中。".into());
        }
    }
}

fn push_string_or_array(target: &mut Vec<(String, String)>, key: &str, value: Option<&Value>) {
    match value {
        Some(Value::String(s)) if !s.trim().is_empty() => {
            target.push((key.into(), s.trim().to_owned()));
        }
        Some(Value::Array(items)) => {
            let joined: Vec<&str> = items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if !joined.is_empty() {
                target.push((key.into(), joined.join(",")));
            }
        }
        _ => {}
    }
}

fn append_stream_query_params(
    stream: Option<&Value>,
    default_security: &str,
    params: &mut Vec<(String, String)>,
    warnings: &mut Vec<String>,
) {
    let Some(stream) = stream else {
        return;
    };
    let network = non_empty_str(stream.get("network"))
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "tcp".into());
    let security = non_empty_str(stream.get("security"))
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| default_security.into());

    params.push(("type".into(), network.clone()));
    params.push(("security".into(), security.clone()));

    match security.as_str() {
        "tls" => {
            let tls = stream.get("tlsSettings");
            if let Some(sni) = non_empty_str(tls.and_then(|t| t.get("serverName"))) {
                params.push(("sni".into(), sni));
            }
            if let Some(fp) = non_empty_str(tls.and_then(|t| t.get("fingerprint"))) {
                params.push(("fp".into(), fp));
            }
            push_string_or_array(params, "alpn", tls.and_then(|t| t.get("alpn")));
            if tls
                .and_then(|t| t.get("allowInsecure"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                params.push(("allowInsecure".into(), "1".into()));
            }
        }
        "reality" => {
            let reality = stream.get("realitySettings");
            if let Some(sni) = non_empty_str(reality.and_then(|r| r.get("serverName"))) {
                params.push(("sni".into(), sni));
            }
            if let Some(fp) = non_empty_str(reality.and_then(|r| r.get("fingerprint"))) {
                params.push(("fp".into(), fp));
            }
            if let Some(pbk) = non_empty_str(reality.and_then(|r| r.get("publicKey"))) {
                params.push(("pbk".into(), pbk));
            }
            if let Some(sid) = non_empty_str(reality.and_then(|r| r.get("shortId"))) {
                params.push(("sid".into(), sid));
            }
            if let Some(spx) = non_empty_str(reality.and_then(|r| r.get("spiderX"))) {
                params.push(("spx".into(), spx));
            }
            if let Some(pqv) = non_empty_str(reality.and_then(|r| r.get("mldsa65Verify"))) {
                params.push(("pqv".into(), pqv));
            }
        }
        "none" => {}
        other => {
            warnings.push(format!(
                "安全类型 security={other} 的专有参数可能无法完整表达。"
            ));
        }
    }

    match network.as_str() {
        "ws" => {
            let ws = stream.get("wsSettings");
            if let Some(path) = ws
                .and_then(|w| w.get("path"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                params.push(("path".into(), path.to_owned()));
            }
            if let Some(host) = non_empty_str(
                ws.and_then(|w| w.pointer("/headers/Host"))
                    .or_else(|| ws.and_then(|w| w.get("host"))),
            ) {
                params.push(("host".into(), host));
            }
        }
        "grpc" => {
            let grpc = stream.get("grpcSettings");
            if let Some(service) = non_empty_str(grpc.and_then(|g| g.get("serviceName"))) {
                params.push(("serviceName".into(), service));
            }
            if grpc
                .and_then(|g| g.get("multiMode"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                params.push(("mode".into(), "multi".into()));
            }
        }
        "httpupgrade" => {
            let hu = stream.get("httpupgradeSettings");
            if let Some(path) = hu
                .and_then(|h| h.get("path"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                params.push(("path".into(), path.to_owned()));
            }
            if let Some(host) = non_empty_str(hu.and_then(|h| h.get("host"))) {
                params.push(("host".into(), host));
            }
        }
        "splithttp" | "xhttp" => {
            let xh = if network == "splithttp" {
                stream
                    .get("splithttpSettings")
                    .or_else(|| stream.get("xhttpSettings"))
            } else {
                stream
                    .get("xhttpSettings")
                    .or_else(|| stream.get("splithttpSettings"))
            };
            if let Some(path) = xh
                .and_then(|x| x.get("path"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                params.push(("path".into(), path.to_owned()));
            }
            if let Some(host) = non_empty_str(xh.and_then(|x| x.get("host"))) {
                params.push(("host".into(), host));
            }
            if let Some(mode) = non_empty_str(xh.and_then(|x| x.get("mode"))) {
                params.push(("mode".into(), mode));
            }
        }
        "kcp" => {
            let kcp = stream.get("kcpSettings");
            if let Some(seed) = kcp
                .and_then(|k| k.get("seed"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                params.push(("seed".into(), seed.to_owned()));
            }
            if let Some(ht) = non_empty_str(kcp.and_then(|k| k.pointer("/header/type"))) {
                params.push(("headerType".into(), ht));
            }
        }
        "quic" => {
            let quic = stream.get("quicSettings");
            if let Some(sec) = non_empty_str(quic.and_then(|q| q.get("security"))) {
                params.push(("quicSecurity".into(), sec));
            }
            if let Some(key) = quic
                .and_then(|q| q.get("key"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                params.push(("key".into(), key.to_owned()));
            }
            if let Some(ht) = non_empty_str(quic.and_then(|q| q.pointer("/header/type"))) {
                params.push(("headerType".into(), ht));
            }
        }
        "tcp" | "raw" => {
            let tcp = if network == "raw" {
                stream
                    .get("rawSettings")
                    .or_else(|| stream.get("tcpSettings"))
            } else {
                stream
                    .get("tcpSettings")
                    .or_else(|| stream.get("rawSettings"))
            };
            if non_empty_str(tcp.and_then(|t| t.pointer("/header/type")))
                .is_some_and(|h| h.eq_ignore_ascii_case("http"))
            {
                params.push(("headerType".into(), "http".into()));
                push_string_or_array(
                    params,
                    "path",
                    tcp.and_then(|t| t.pointer("/header/request/path")),
                );
                push_string_or_array(
                    params,
                    "host",
                    tcp.and_then(|t| t.pointer("/header/request/headers/Host")),
                );
            }
        }
        other => {
            warnings.push(format!(
                "传输方式 {other} 的专有参数未自动编码进分享链接，已按 type={other} 保留。"
            ));
        }
    }
}

/// Encodes a single Xray outbound object into a standard proxy share URI.
pub fn encode_share_link(
    outbound: &Value,
    display_name: Option<&str>,
) -> std::result::Result<EncodedShareNode, String> {
    let protocol = non_empty_str(outbound.get("protocol"))
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| "节点缺少协议字段（protocol）".to_owned())?;
    let tag = non_empty_str(outbound.get("tag")).unwrap_or_default();
    let name = display_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| tag.clone());

    let mut warnings = Vec::new();
    collect_lossy_outbound_warnings(outbound, &mut warnings);

    let uri = match protocol.as_str() {
        "vless" => encode_vless(outbound, &name, &mut warnings)?,
        "trojan" => encode_trojan(outbound, &name, &mut warnings)?,
        "shadowsocks" => encode_ss(outbound, &name, &mut warnings)?,
        "vmess" => encode_vmess(outbound, &name, &mut warnings)?,
        "socks" => encode_upstream(outbound, "socks", &name, &mut warnings)?,
        "http" => encode_upstream(outbound, "http", &name, &mut warnings)?,
        other => return Err(format!("暂不支持导出 {other} 协议的标准分享链接")),
    };

    Ok(EncodedShareNode {
        index: 0,
        tag,
        name,
        protocol,
        uri,
        warnings,
    })
}

fn encode_vless(
    outbound: &Value,
    name: &str,
    warnings: &mut Vec<String>,
) -> std::result::Result<String, String> {
    let server = outbound
        .pointer("/settings/vnext/0")
        .ok_or_else(|| "VLESS 节点缺少 vnext 服务器配置".to_owned())?;
    let address = non_empty_str(server.get("address"))
        .ok_or_else(|| "VLESS 节点缺少服务器地址".to_owned())?;
    let port = required_outbound_port(server.get("port"))?;
    let user = server
        .pointer("/users/0")
        .ok_or_else(|| "VLESS 节点缺少用户信息".to_owned())?;
    let uuid =
        non_empty_str(user.get("id")).ok_or_else(|| "VLESS 节点缺少用户 ID / UUID".to_owned())?;

    let mut params = Vec::new();
    let encryption = non_empty_str(user.get("encryption")).unwrap_or_else(|| "none".into());
    params.push(("encryption".into(), encryption));
    if let Some(flow) = non_empty_str(user.get("flow")) {
        params.push(("flow".into(), flow));
    }
    append_stream_query_params(
        outbound.get("streamSettings"),
        "none",
        &mut params,
        warnings,
    );
    let query = encode_query_pairs(&params);
    let authority = format_authority_host(&address);
    let frag = encode_fragment(name);
    if query.is_empty() {
        Ok(format!(
            "vless://{}@{authority}:{port}{frag}",
            encode_userinfo(&uuid)
        ))
    } else {
        Ok(format!(
            "vless://{}@{authority}:{port}?{query}{frag}",
            encode_userinfo(&uuid)
        ))
    }
}

fn encode_trojan(
    outbound: &Value,
    name: &str,
    warnings: &mut Vec<String>,
) -> std::result::Result<String, String> {
    let server = outbound
        .pointer("/settings/servers/0")
        .ok_or_else(|| "Trojan 节点缺少 servers 服务器配置".to_owned())?;
    let address = non_empty_str(server.get("address"))
        .ok_or_else(|| "Trojan 节点缺少服务器地址".to_owned())?;
    let port = required_outbound_port(server.get("port"))?;
    let password = server
        .get("password")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Trojan 节点缺少密码".to_owned())?;

    let mut params = Vec::new();
    append_stream_query_params(outbound.get("streamSettings"), "tls", &mut params, warnings);
    let query = encode_query_pairs(&params);
    let authority = format_authority_host(&address);
    let frag = encode_fragment(name);
    if query.is_empty() {
        Ok(format!(
            "trojan://{}@{authority}:{port}{frag}",
            encode_userinfo(password)
        ))
    } else {
        Ok(format!(
            "trojan://{}@{authority}:{port}?{query}{frag}",
            encode_userinfo(password)
        ))
    }
}

fn encode_ss(
    outbound: &Value,
    name: &str,
    warnings: &mut Vec<String>,
) -> std::result::Result<String, String> {
    let server = outbound
        .pointer("/settings/servers/0")
        .ok_or_else(|| "Shadowsocks 节点缺少 servers 服务器配置".to_owned())?;
    let address = non_empty_str(server.get("address"))
        .ok_or_else(|| "Shadowsocks 节点缺少服务器地址".to_owned())?;
    let port = required_outbound_port(server.get("port"))?;
    let method = non_empty_str(server.get("method"))
        .ok_or_else(|| "Shadowsocks 节点缺少加密方式 (method)".to_owned())?;
    let password = server
        .get("password")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Shadowsocks 节点缺少密码".to_owned())?;

    if let Some(stream) = outbound.get("streamSettings") {
        let net = non_empty_str(stream.get("network")).unwrap_or_else(|| "tcp".into());
        let sec = non_empty_str(stream.get("security")).unwrap_or_else(|| "none".into());
        if !net.eq_ignore_ascii_case("tcp") || !sec.eq_ignore_ascii_case("none") {
            warnings.push("Shadowsocks 标准 SIP002 链接不包含流传输/TLS 扩展配置，已省略。".into());
        }
    }

    let creds =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{method}:{password}"));
    let authority = format_authority_host(&address);
    let frag = encode_fragment(name);
    Ok(format!("ss://{creds}@{authority}:{port}{frag}"))
}

fn encode_vmess(
    outbound: &Value,
    name: &str,
    warnings: &mut Vec<String>,
) -> std::result::Result<String, String> {
    let server = outbound
        .pointer("/settings/vnext/0")
        .ok_or_else(|| "VMess 节点缺少 vnext 服务器配置".to_owned())?;
    let address = non_empty_str(server.get("address"))
        .ok_or_else(|| "VMess 节点缺少服务器地址".to_owned())?;
    let port = required_outbound_port(server.get("port"))?;
    let user = server
        .pointer("/users/0")
        .ok_or_else(|| "VMess 节点缺少用户信息".to_owned())?;
    let id = non_empty_str(user.get("id")).ok_or_else(|| "VMess 节点缺少用户 ID".to_owned())?;
    let scy = non_empty_str(user.get("security")).unwrap_or_else(|| "auto".into());
    let aid = user
        .get("alterId")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .to_string();

    let stream = outbound.get("streamSettings");
    let net = stream
        .and_then(|s| non_empty_str(s.get("network")))
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "tcp".into());
    let security = stream
        .and_then(|s| non_empty_str(s.get("security")))
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "none".into());

    let mut obj = serde_json::Map::new();
    obj.insert("v".into(), json!("2"));
    obj.insert("ps".into(), json!(name));
    obj.insert("add".into(), json!(address));
    obj.insert("port".into(), json!(port.to_string()));
    obj.insert("id".into(), json!(id));
    obj.insert("aid".into(), json!(aid));
    obj.insert("scy".into(), json!(scy));
    obj.insert("net".into(), json!(net));

    match security.as_str() {
        "tls" => {
            obj.insert("tls".into(), json!("tls"));
            let tls = stream.and_then(|s| s.get("tlsSettings"));
            if let Some(sni) = non_empty_str(tls.and_then(|t| t.get("serverName"))) {
                obj.insert("sni".into(), json!(sni));
            }
            if let Some(fp) = non_empty_str(tls.and_then(|t| t.get("fingerprint"))) {
                obj.insert("fp".into(), json!(fp));
            }
            if let Some(alpn) = tls.and_then(|t| t.get("alpn")) {
                match alpn {
                    Value::String(s) if !s.trim().is_empty() => {
                        obj.insert("alpn".into(), json!(s.trim()));
                    }
                    Value::Array(items) => {
                        let joined: Vec<&str> = items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .collect();
                        if !joined.is_empty() {
                            obj.insert("alpn".into(), json!(joined.join(",")));
                        }
                    }
                    _ => {}
                }
            }
            if tls
                .and_then(|t| t.get("allowInsecure"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                obj.insert("allowInsecure".into(), json!(1));
            }
        }
        "reality" => {
            warnings.push(
                "VMess 标准分享链接不支持 Reality 安全参数，已省略；如需完整保留请导出配置包。"
                    .into(),
            );
        }
        "none" => {}
        other => {
            warnings.push(format!("VMess 分享链接不支持安全类型 {other}，已省略。"));
        }
    }

    if let Some(stream) = stream {
        match net.as_str() {
            "ws" => {
                let ws = stream.get("wsSettings");
                if let Some(path) = ws
                    .and_then(|w| w.get("path"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    obj.insert("path".into(), json!(path));
                }
                if let Some(host) = non_empty_str(
                    ws.and_then(|w| w.pointer("/headers/Host"))
                        .or_else(|| ws.and_then(|w| w.get("host"))),
                ) {
                    obj.insert("host".into(), json!(host));
                }
            }
            "grpc" => {
                let grpc = stream.get("grpcSettings");
                if let Some(service) = non_empty_str(grpc.and_then(|g| g.get("serviceName"))) {
                    obj.insert("path".into(), json!(service));
                }
                if grpc
                    .and_then(|g| g.get("multiMode"))
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    obj.insert("type".into(), json!("multi"));
                }
            }
            "httpupgrade" => {
                let hu = stream.get("httpupgradeSettings");
                if let Some(path) = hu
                    .and_then(|h| h.get("path"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    obj.insert("path".into(), json!(path));
                }
                if let Some(host) = non_empty_str(hu.and_then(|h| h.get("host"))) {
                    obj.insert("host".into(), json!(host));
                }
            }
            "kcp" => {
                let kcp = stream.get("kcpSettings");
                if let Some(seed) = kcp
                    .and_then(|k| k.get("seed"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    obj.insert("path".into(), json!(seed));
                }
                if let Some(ht) = non_empty_str(kcp.and_then(|k| k.pointer("/header/type"))) {
                    obj.insert("type".into(), json!(ht));
                }
            }
            "quic" => {
                let quic = stream.get("quicSettings");
                if let Some(sec) = non_empty_str(quic.and_then(|q| q.get("security"))) {
                    obj.insert("host".into(), json!(sec));
                }
                if let Some(key) = quic
                    .and_then(|q| q.get("key"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    obj.insert("path".into(), json!(key));
                }
                if let Some(ht) = non_empty_str(quic.and_then(|q| q.pointer("/header/type"))) {
                    obj.insert("type".into(), json!(ht));
                }
            }
            "tcp" | "raw" => {
                let tcp = if net == "raw" {
                    stream
                        .get("rawSettings")
                        .or_else(|| stream.get("tcpSettings"))
                } else {
                    stream
                        .get("tcpSettings")
                        .or_else(|| stream.get("rawSettings"))
                };
                if non_empty_str(tcp.and_then(|t| t.pointer("/header/type")))
                    .is_some_and(|h| h.eq_ignore_ascii_case("http"))
                {
                    obj.insert("type".into(), json!("http"));
                    let mut tmp = Vec::new();
                    push_string_or_array(
                        &mut tmp,
                        "path",
                        tcp.and_then(|t| t.pointer("/header/request/path")),
                    );
                    push_string_or_array(
                        &mut tmp,
                        "host",
                        tcp.and_then(|t| t.pointer("/header/request/headers/Host")),
                    );
                    for (k, v) in tmp {
                        obj.insert(k, json!(v));
                    }
                }
            }
            other => {
                warnings.push(format!(
                    "VMess 标准链接对传输方式 {other} 的专有参数支持有限，已省略高级字段。"
                ));
            }
        }
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(Value::Object(obj).to_string());
    Ok(format!("vmess://{encoded}"))
}

fn encode_upstream(
    outbound: &Value,
    protocol: &str,
    name: &str,
    warnings: &mut Vec<String>,
) -> std::result::Result<String, String> {
    let server = outbound
        .pointer("/settings/servers/0")
        .ok_or_else(|| format!("{protocol} 节点缺少 servers 服务器配置"))?;
    let address = non_empty_str(server.get("address"))
        .ok_or_else(|| format!("{protocol} 节点缺少服务器地址"))?;
    let port = required_outbound_port(server.get("port"))?;

    let user_entry = server.pointer("/users/0");
    let user = non_empty_str(user_entry.and_then(|u| u.get("user")));
    let pass = user_entry
        .and_then(|u| u.get("pass"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let auth_prefix = match user {
        Some(u) if !pass.is_empty() => {
            format!("{}:{}@", encode_userinfo(&u), encode_userinfo(pass))
        }
        Some(u) => format!("{}@", encode_userinfo(&u)),
        None => String::new(),
    };

    let stream = outbound.get("streamSettings");
    let security = stream
        .and_then(|s| non_empty_str(s.get("security")))
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "none".into());
    let scheme = if protocol == "http" && security == "tls" {
        "https"
    } else {
        protocol
    };

    let mut params = Vec::new();
    append_stream_query_params(
        stream,
        if scheme == "https" { "tls" } else { "none" },
        &mut params,
        warnings,
    );
    let query = encode_query_pairs(&params);
    let authority = format_authority_host(&address);
    let frag = encode_fragment(name);
    if query.is_empty() {
        Ok(format!("{scheme}://{auth_prefix}{authority}:{port}{frag}"))
    } else {
        Ok(format!(
            "{scheme}://{auth_prefix}{authority}:{port}?{query}{frag}"
        ))
    }
}

/// Encodes all shareable outbounds in an Xray configuration document.
pub fn encode_share_links(
    config: &Value,
    node_names: &BTreeMap<String, String>,
) -> EncodedShareBatch {
    let mut batch = EncodedShareBatch::default();
    let Some(outbounds) = config.get("outbounds").and_then(Value::as_array) else {
        return batch;
    };
    for (index, outbound) in outbounds.iter().enumerate() {
        let tag = non_empty_str(outbound.get("tag")).unwrap_or_default();
        let protocol = non_empty_str(outbound.get("protocol"))
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(protocol.as_str(), "freedom" | "blackhole" | "dns") {
            batch.skipped.push((
                if tag.is_empty() {
                    format!("#{index}")
                } else {
                    tag
                },
                format!("内置系统出站（{protocol}）"),
            ));
            continue;
        }
        let display_name = node_names
            .get(&tag)
            .map(String::as_str)
            .filter(|s| !s.trim().is_empty());
        match encode_share_link(outbound, display_name) {
            Ok(mut node) => {
                node.index = index;
                batch.nodes.push(node);
            }
            Err(err) => {
                batch.errors.push((
                    if tag.is_empty() {
                        format!("出站 #{index}")
                    } else {
                        tag
                    },
                    err,
                ));
            }
        }
    }
    batch
}

/// Formats encoded share nodes as newline-separated URIs or a Base64-encoded subscription block.
pub fn format_share_links_text(nodes: &[EncodedShareNode], as_base64: bool) -> String {
    let joined = nodes
        .iter()
        .map(|n| n.uri.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if as_base64 && !joined.is_empty() {
        base64::engine::general_purpose::STANDARD.encode(joined.as_bytes())
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vless_reality_link() {
        let link = "vless://11111111-2222-3333-4444-555555555555@example.com:443?encryption=none&security=reality&sni=www.microsoft.com&fp=chrome&pbk=TEST_PBK&sid=abcd&flow=xtls-rprx-vision&type=tcp#Tokyo%20Reality";
        let node = parse_share_link(link).unwrap();
        assert_eq!(node.protocol, "vless");
        assert_eq!(node.name.as_deref(), Some("Tokyo Reality"));
        assert!(node.warnings.is_empty());
        assert_eq!(
            node.outbound["settings"]["vnext"][0]["address"],
            "example.com"
        );
        assert_eq!(node.outbound["settings"]["vnext"][0]["port"], 443);
        assert_eq!(
            node.outbound["settings"]["vnext"][0]["users"][0]["id"],
            "11111111-2222-3333-4444-555555555555"
        );
        assert_eq!(
            node.outbound["settings"]["vnext"][0]["users"][0]["flow"],
            "xtls-rprx-vision"
        );
        assert_eq!(node.outbound["streamSettings"]["security"], "reality");
        assert_eq!(
            node.outbound["streamSettings"]["realitySettings"]["publicKey"],
            "TEST_PBK"
        );
        assert_eq!(
            node.outbound["streamSettings"]["realitySettings"]["shortId"],
            "abcd"
        );
        assert_eq!(
            node.outbound["streamSettings"]["realitySettings"]["fingerprint"],
            "chrome"
        );
    }

    #[test]
    fn parses_trojan_ws_with_allow_insecure_warning() {
        let link = "trojan://secret-pass@[2001:db8::1]:8443?security=tls&type=ws&path=%2Fws&host=cdn.example.com&sni=cdn.example.com&allowInsecure=1#HK-Trojan";
        let node = parse_share_link(link).unwrap();
        assert_eq!(node.protocol, "trojan");
        assert_eq!(node.name.as_deref(), Some("HK-Trojan"));
        assert_eq!(
            node.outbound["settings"]["servers"][0]["address"],
            "2001:db8::1"
        );
        assert_eq!(node.outbound["settings"]["servers"][0]["port"], 8443);
        assert_eq!(node.outbound["streamSettings"]["network"], "ws");
        assert_eq!(node.outbound["streamSettings"]["wsSettings"]["path"], "/ws");
        assert_eq!(
            node.outbound["streamSettings"]["wsSettings"]["headers"]["Host"],
            "cdn.example.com"
        );
        assert_eq!(
            node.outbound["streamSettings"]["tlsSettings"]["allowInsecure"],
            true
        );
        assert_eq!(node.warnings.len(), 1);
    }

    #[test]
    fn parses_ss_sip002_and_legacy_formats() {
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("aes-256-gcm:my-pass");
        let sip002 = format!("ss://{b64}@ss.example.com:8388#SS-Node");
        let node = parse_share_link(&sip002).unwrap();
        assert_eq!(node.protocol, "shadowsocks");
        assert_eq!(node.name.as_deref(), Some("SS-Node"));
        assert_eq!(
            node.outbound["settings"]["servers"][0]["method"],
            "aes-256-gcm"
        );
        assert_eq!(
            node.outbound["settings"]["servers"][0]["password"],
            "my-pass"
        );

        let legacy_b64 = base64::engine::general_purpose::STANDARD
            .encode("chacha20-ietf-poly1305:legacy-pass@1.2.3.4:9000");
        let legacy = format!("ss://{legacy_b64}#Legacy");
        let node2 = parse_share_link(&legacy).unwrap();
        assert_eq!(
            node2.outbound["settings"]["servers"][0]["address"],
            "1.2.3.4"
        );
        assert_eq!(node2.outbound["settings"]["servers"][0]["port"], 9000);
        assert_eq!(node2.name.as_deref(), Some("Legacy"));
    }

    #[test]
    fn parses_vmess_base64_and_batch_subscription() {
        let vmess_json = json!({
            "v": "2",
            "ps": "VMess-GRPC",
            "add": "vmess.example.com",
            "port": "443",
            "id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            "aid": "0",
            "scy": "auto",
            "net": "grpc",
            "path": "grpc-svc",
            "tls": "tls",
            "sni": "vmess.example.com"
        });
        let vmess = format!(
            "vmess://{}",
            base64::engine::general_purpose::STANDARD.encode(vmess_json.to_string())
        );
        let raw_list = format!("{vmess}\n# comment\nsocks://alice:bob@127.0.0.1:1080#LocalSocks");
        let encoded_list = base64::engine::general_purpose::STANDARD.encode(&raw_list);

        let batch = parse_share_links(&encoded_list);
        assert!(batch.errors.is_empty());
        assert_eq!(batch.nodes.len(), 2);
        assert_eq!(batch.nodes[0].name.as_deref(), Some("VMess-GRPC"));
        assert_eq!(
            batch.nodes[0].outbound["streamSettings"]["grpcSettings"]["serviceName"],
            "grpc-svc"
        );
        assert_eq!(batch.nodes[1].protocol, "socks");
        assert_eq!(
            batch.nodes[1].outbound["settings"]["servers"][0]["users"][0]["user"],
            "alice"
        );
    }

    #[test]
    fn rejects_incomplete_or_unsupported_links() {
        assert!(parse_share_link("vless://@example.com:443").is_err());
        assert!(parse_share_link("vless://uuid@example.com").is_err());
        assert!(parse_share_link("hysteria2://pass@example.com:443").is_err());
    }

    #[test]
    fn encodes_and_roundtrips_all_supported_protocols() {
        let cases = [
            (
                "Tokyo Reality",
                json!({
                    "tag": "node-vless",
                    "protocol": "vless",
                    "settings": {
                        "vnext": [{
                            "address": "example.com",
                            "port": 443,
                            "users": [{"id": "11111111-2222-3333-4444-555555555555", "encryption": "none", "flow": "xtls-rprx-vision"}]
                        }]
                    },
                    "streamSettings": {
                        "network": "tcp",
                        "security": "reality",
                        "realitySettings": {
                            "serverName": "www.microsoft.com",
                            "fingerprint": "chrome",
                            "publicKey": "TEST_PBK",
                            "shortId": "abcd",
                            "spiderX": "/v1"
                        }
                    }
                }),
            ),
            (
                "VLESS WS TLS",
                json!({
                    "tag": "node-ws",
                    "protocol": "vless",
                    "settings": {
                        "vnext": [{
                            "address": "ws.example.com",
                            "port": 443,
                            "users": [{"id": "22222222-2222-3333-4444-555555555555", "encryption": "none"}]
                        }]
                    },
                    "streamSettings": {
                        "network": "ws",
                        "security": "tls",
                        "tlsSettings": {"serverName": "ws.example.com", "alpn": ["h2", "http/1.1"]},
                        "wsSettings": {"path": "/ws?ed=2048", "headers": {"Host": "cdn.example.com"}}
                    }
                }),
            ),
            (
                "VLESS xhttp",
                json!({
                    "tag": "node-xhttp",
                    "protocol": "vless",
                    "settings": {
                        "vnext": [{
                            "address": "xhttp.example.com",
                            "port": 443,
                            "users": [{"id": "33333333-2222-3333-4444-555555555555", "encryption": "none"}]
                        }]
                    },
                    "streamSettings": {
                        "network": "xhttp",
                        "security": "tls",
                        "tlsSettings": {"serverName": "xhttp.example.com"},
                        "xhttpSettings": {"path": "/xh", "host": "xhttp.example.com", "mode": "auto"}
                    }
                }),
            ),
            (
                "HK Trojan",
                json!({
                    "tag": "node-trojan",
                    "protocol": "trojan",
                    "settings": {
                        "servers": [{"address": "2001:db8::1", "port": 8443, "password": "secret/pass?1"}]
                    },
                    "streamSettings": {
                        "network": "grpc",
                        "security": "tls",
                        "tlsSettings": {"serverName": "cdn.example.com", "fingerprint": "safari"},
                        "grpcSettings": {"serviceName": "trojan-grpc", "multiMode": true}
                    }
                }),
            ),
            (
                "SS Node",
                json!({
                    "tag": "node-ss",
                    "protocol": "shadowsocks",
                    "settings": {
                        "servers": [{"address": "ss.example.com", "port": 8388, "method": "aes-256-gcm", "password": "pass:with#chars"}]
                    }
                }),
            ),
            (
                "VMess Node",
                json!({
                    "tag": "node-vmess",
                    "protocol": "vmess",
                    "settings": {
                        "vnext": [{
                            "address": "vmess.example.com",
                            "port": 443,
                            "users": [{"id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", "security": "auto", "alterId": 2}]
                        }]
                    },
                    "streamSettings": {
                        "network": "ws",
                        "security": "tls",
                        "tlsSettings": {"serverName": "vmess.example.com", "allowInsecure": true},
                        "wsSettings": {"path": "/vm", "headers": {"Host": "vmess.example.com"}}
                    }
                }),
            ),
            (
                "Local Socks",
                json!({
                    "tag": "node-socks",
                    "protocol": "socks",
                    "settings": {
                        "servers": [{"address": "127.0.0.1", "port": 1080, "users": [{"user": "alice", "pass": "bob@1"}]}]
                    }
                }),
            ),
            (
                "HTTPS Upstream",
                json!({
                    "tag": "node-https",
                    "protocol": "http",
                    "settings": {
                        "servers": [{"address": "proxy.example.com", "port": 443, "users": [{"user": "u1", "pass": "p1"}]}]
                    },
                    "streamSettings": {
                        "network": "tcp",
                        "security": "tls",
                        "tlsSettings": {"serverName": "proxy.example.com"}
                    }
                }),
            ),
        ];

        for (display, outbound) in cases {
            let encoded = encode_share_link(&outbound, Some(display)).unwrap();
            let parsed = parse_share_link(&encoded.uri).unwrap();
            assert_eq!(parsed.name.as_deref(), Some(display), "URI={}", encoded.uri);
            assert_eq!(parsed.protocol, outbound["protocol"].as_str().unwrap());
            assert_eq!(
                parsed.outbound["settings"], outbound["settings"],
                "URI={}",
                encoded.uri
            );
            if outbound.get("streamSettings").is_some() {
                assert_eq!(
                    parsed.outbound["streamSettings"], outbound["streamSettings"],
                    "URI={}",
                    encoded.uri
                );
            }
        }
    }

    #[test]
    fn encode_share_links_skips_builtins_and_reports_lossy_fields() {
        let config = json!({
            "outbounds": [
                {
                    "tag": "node-1",
                    "protocol": "vless",
                    "mux": {"enabled": true},
                    "settings": {
                        "vnext": [{
                            "address": "example.com",
                            "port": 443,
                            "users": [
                                {"id": "uuid-1", "encryption": "none"},
                                {"id": "uuid-2", "encryption": "none"}
                            ]
                        }]
                    },
                    "streamSettings": {
                        "network": "tcp",
                        "security": "none",
                        "sockopt": {"mark": 255}
                    }
                },
                {"tag": "direct", "protocol": "freedom", "settings": {}},
                {"tag": "block", "protocol": "blackhole", "settings": {}}
            ]
        });
        let names = BTreeMap::from([("node-1".into(), "主节点".into())]);
        let batch = encode_share_links(&config, &names);
        assert_eq!(batch.nodes.len(), 1);
        assert_eq!(batch.nodes[0].name, "主节点");
        assert_eq!(batch.nodes[0].index, 0);
        assert_eq!(batch.skipped.len(), 2);
        assert!(batch.errors.is_empty());
        assert!(batch.nodes[0].warnings.iter().any(|w| w.contains("mux")));
        assert!(
            batch.nodes[0]
                .warnings
                .iter()
                .any(|w| w.contains("sockopt"))
        );
        assert!(batch.nodes[0].warnings.iter().any(|w| w.contains("users")));

        let plain = format_share_links_text(&batch.nodes, false);
        let b64 = format_share_links_text(&batch.nodes, true);
        assert_eq!(parse_share_links(&plain).nodes.len(), 1);
        assert_eq!(parse_share_links(&b64).nodes.len(), 1);
    }
}
