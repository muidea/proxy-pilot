//! Offline parser for standard proxy share URIs (`vless://`, `trojan://`, `ss://`, `vmess://`, `socks://`, `http://`).
//! Never performs network I/O and never fabricates missing remote credentials or addresses.

use base64::Engine as _;
use percent_encoding::percent_decode_str;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use url::Url;

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
}
