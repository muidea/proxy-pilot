use proxy_model::{Error, Result};
use serde_json::Value;
use std::collections::HashSet;

/// Contains field locations and remediation, never copied credentials or server settings.
pub struct ConfigIssue {
    pub path: String,
    pub message: String,
    pub help: &'static str,
    pub blocking: bool,
    pub code: &'static str,
}

/// Import accepts incomplete native objects without inventing missing configuration.
pub fn parse_document(bytes: &[u8]) -> Result<Value> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(Error::new(
            "CONFIG_TOO_LARGE",
            "Configuration exceeds 4 MiB",
        ));
    }
    let value: Value = serde_json::from_slice(bytes)?;
    if !value.is_object() {
        return Err(Error::new(
            "CONFIG_PARSE_FAILED",
            "Configuration must be a JSON object",
        ));
    }
    Ok(value)
}

/// Conservative checks of known fields. The selected official Core remains the
/// authority for native protocols and their complete version-specific semantics.
pub fn inspect_config(value: &Value) -> Vec<ConfigIssue> {
    let mut issues = vec![];
    let mut push = |path: String, message: String, help, blocking, code| {
        issues.push(ConfigIssue {
            path,
            message,
            help,
            blocking,
            code,
        });
    };
    for name in ["inbounds", "outbounds"] {
        let Some(items) = value.get(name) else {
            push(
                format!("/{name}"),
                format!("未显式配置 {name}"),
                if name == "inbounds" {
                    "如需本机应用连接，请补充所需 HTTP / SOCKS 等入口；特殊用途配置请核对 Core 的默认行为。"
                } else {
                    "请确认默认出站符合预期；未显式配置不等于原生配置非法，最终以所选 Core 校验为准。"
                },
                false,
                "CONFIG_REVIEW",
            );
            continue;
        };
        let Some(items) = items.as_array() else {
            push(
                format!("/{name}"),
                format!("{name} 应为数组"),
                "保留原内容，将此字段修正为 JSON 数组后再检查。",
                true,
                "STATIC_VALIDATION_FAILED",
            );
            continue;
        };
        if items.is_empty() {
            push(
                format!("/{name}"),
                format!("{name} 数组为空"),
                "请确认是否需要补充入口或出站；特殊用途配置以 Core 校验和实际用途为准。",
                false,
                "CONFIG_REVIEW",
            );
        }
        let mut tags = HashSet::new();
        for (index, item) in items.iter().enumerate() {
            let path = format!("/{name}/{index}");
            if !item.is_object() {
                push(
                    path,
                    "条目应为 JSON 对象".into(),
                    "补充原生协议对象，或在原始 JSON 中移除此无效条目。",
                    true,
                    "STATIC_VALIDATION_FAILED",
                );
                continue;
            }
            if let Some(tag) = item.get("tag").and_then(Value::as_str)
                && !tags.insert(tag)
            {
                push(
                    format!("{path}/tag"),
                    "同一列表中的 Tag 重复".into(),
                    "为重复条目使用唯一 Tag，并同步核对引用它的路由。",
                    true,
                    "STATIC_VALIDATION_FAILED",
                );
            }
            match item.get("protocol").and_then(Value::as_str) {
                None | Some("") => push(
                    format!("{path}/protocol"),
                    "缺少协议名称".into(),
                    "按原节点或入口资料填写 protocol，不自动猜测协议或节点参数。",
                    true,
                    "STATIC_VALIDATION_FAILED",
                ),
                Some("tun") if name == "inbounds" => push(
                    format!("{path}/protocol"),
                    "当前版本尚不支持托管 TUN".into(),
                    "可保留配置，但启用 TUN 需要后续特权 Helper 支持。",
                    true,
                    "UNSUPPORTED_CAPABILITY",
                ),
                _ => {}
            }
        }
    }
    let api_tag = value.pointer("/api/tag").and_then(Value::as_str);
    if let Some(inbounds) = value.get("inbounds").and_then(Value::as_array) {
        for (conflict_index, message) in inbound_port_conflicts(inbounds) {
            push(
                format!("/inbounds/{conflict_index}/port"),
                message,
                "调整本机监听地址或端口，避免同一协议端口与已有入口或其它端口转发冲突。",
                true,
                "STATIC_VALIDATION_FAILED",
            );
        }
        for (index, item) in inbounds.iter().enumerate() {
            if item.get("protocol").and_then(Value::as_str) == Some("dokodemo-door")
                && api_tag != item.get("tag").and_then(Value::as_str)
            {
                let listen = item
                    .get("listen")
                    .and_then(Value::as_str)
                    .unwrap_or("0.0.0.0");
                let loopback = listen
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback());
                if !loopback {
                    push(
                        format!("/inbounds/{index}/listen"),
                        "dokodemo-door 监听非回环地址时无密码认证保护".into(),
                        "如仅本机使用建议设为 127.0.0.1；使用 0.0.0.0 时请确认局域网访问安全。",
                        false,
                        "CONFIG_REVIEW",
                    );
                }
            }
        }
    }
    let mut targets: HashSet<&str> = value
        .get("outbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.get("tag").and_then(Value::as_str))
        .collect();
    // API services create an internal outbound handler; it is deliberately absent
    // from outbounds. Reverse portals likewise provide a native outbound target.
    if let Some(tag) = value
        .pointer("/api/tag")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
    {
        targets.insert(tag);
    }
    if let Some(portals) = value.pointer("/reverse/portals").and_then(Value::as_array) {
        targets.extend(
            portals
                .iter()
                .filter_map(|p| p.get("tag").and_then(Value::as_str)),
        );
    }
    if let Some(rules) = value.pointer("/routing/rules").and_then(Value::as_array) {
        for (index, rule) in rules.iter().enumerate() {
            if let Some(tag) = rule.get("outboundTag").and_then(Value::as_str)
                && !targets.contains(tag)
            {
                push(
                    format!("/routing/rules/{index}/outboundTag"),
                    format!("未找到路由目标：{tag}"),
                    "补充对应的 outbounds 条目；若为 API / 反向代理内部路由，核对 api.tag / reverse.portals 的声明。不要替换成直连来绕过缺项。",
                    true,
                    "STATIC_VALIDATION_FAILED",
                );
            }
        }
    }
    issues.extend(remote_parameter_advice(value));
    issues.extend(connection_advice(value));
    issues
}

fn remote_parameter_advice(value: &Value) -> Vec<ConfigIssue> {
    let mut issues = vec![];
    let mut advice = |path: String, field: &str| {
        issues.push(ConfigIssue {
            path,
            message: format!("未找到常用节点参数：{field}"),
            help: concat!(
                "请按节点提供方资料补充；不自动填充地址、端口、密码或用户 ID。",
                "若使用其他版本的原生写法，以所选 Core 校验为准。"
            ),
            blocking: false,
            code: "CONFIG_REVIEW",
        });
    };
    for (index, outbound) in value
        .get("outbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let protocol = outbound
            .get("protocol")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (field, users) = match protocol {
            "vless" | "vmess" => ("vnext", true),
            "trojan" | "shadowsocks" | "socks" | "http" => ("servers", false),
            _ => continue,
        };
        let path = format!("/outbounds/{index}/settings/{field}");
        let Some(servers) = outbound
            .get("settings")
            .and_then(|s| s.get(field))
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty())
        else {
            advice(path, field);
            continue;
        };
        for (server_index, server) in servers.iter().enumerate() {
            let path = format!("{path}/{server_index}");
            if server
                .get("address")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                advice(format!("{path}/address"), "address");
            }
            if server
                .get("port")
                .and_then(Value::as_u64)
                .is_none_or(|p| !(1..=65535).contains(&p))
            {
                advice(format!("{path}/port"), "port");
            }
            if users {
                if let Some(users) = server
                    .get("users")
                    .and_then(Value::as_array)
                    .filter(|a| !a.is_empty())
                {
                    for (user_index, user) in users.iter().enumerate() {
                        if user
                            .get("id")
                            .and_then(Value::as_str)
                            .is_none_or(str::is_empty)
                        {
                            advice(format!("{path}/users/{user_index}/id"), "id");
                        }
                    }
                } else {
                    advice(format!("{path}/users"), "users / id");
                }
            } else if matches!(protocol, "socks" | "http") {
                match server.get("users") {
                    Some(Value::Array(accounts)) => {
                        for (account_index, account) in accounts.iter().enumerate() {
                            if account
                                .get("user")
                                .and_then(Value::as_str)
                                .is_none_or(str::is_empty)
                            {
                                advice(format!("{path}/users/{account_index}/user"), "user");
                            }
                            if account
                                .get("pass")
                                .and_then(Value::as_str)
                                .is_none_or(str::is_empty)
                            {
                                advice(format!("{path}/users/{account_index}/pass"), "pass");
                            }
                        }
                    }
                    Some(_) => advice(format!("{path}/users"), "users / user / pass"),
                    None => {}
                }
            } else if matches!(protocol, "trojan" | "shadowsocks") {
                if server
                    .get("password")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
                {
                    advice(format!("{path}/password"), "password");
                }
                if protocol == "shadowsocks"
                    && server
                        .get("method")
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
                {
                    advice(format!("{path}/method"), "method");
                }
            }
        }
    }
    issues
}

/// Version-dependent connection details are advice, not a competing Core schema.
/// Messages contain paths and field names, never copied authentication values.
fn connection_advice(value: &Value) -> Vec<ConfigIssue> {
    let mut issues = Vec::new();
    let mut advice = |path: String, message: &str, help| {
        issues.push(ConfigIssue {
            path,
            message: message.into(),
            help,
            blocking: false,
            code: "CONFIG_REVIEW",
        });
    };
    for (index, node) in value
        .get("outbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let prefix = format!("/outbounds/{index}/streamSettings");
        if node
            .pointer("/streamSettings/security")
            .and_then(Value::as_str)
            == Some("reality")
        {
            for key in ["serverName", "fingerprint"] {
                if node
                    .pointer(&format!("/streamSettings/realitySettings/{key}"))
                    .and_then(Value::as_str)
                    .is_none_or(|v| v.trim().is_empty())
                {
                    advice(
                        format!("{prefix}/realitySettings/{key}"),
                        "Reality 连接参数待补充 / 核对",
                        "按服务端资料填写服务器名称与客户端指纹；特殊版本写法以所选 Core 校验为准。",
                    );
                }
            }
            if ["publicKey", "password"].iter().all(|key| {
                node.pointer(&format!("/streamSettings/realitySettings/{key}"))
                    .and_then(Value::as_str)
                    .is_none_or(|v| v.trim().is_empty())
            }) {
                advice(
                    format!("{prefix}/realitySettings/publicKey"),
                    "Reality 公钥待补充",
                    "填写服务端提供的公钥，保留原生 publicKey / password 写法，不生成或猜测此值。",
                );
            }
            if let Some(short_id) = node
                .pointer("/streamSettings/realitySettings/shortId")
                .and_then(Value::as_str)
                && (short_id.len() > 16
                    || short_id.len() % 2 != 0
                    || !short_id.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                advice(
                    format!("{prefix}/realitySettings/shortId"),
                    "Short ID 格式需要核对",
                    "使用服务端提供的偶数位十六进制值，最多 16 位；仅在服务端允许时留空。",
                );
            }
        }
        if node
            .pointer("/streamSettings/tlsSettings/allowInsecure")
            .and_then(Value::as_bool)
            == Some(true)
        {
            advice(
                format!("{prefix}/tlsSettings/allowInsecure"),
                "原配置关闭了 TLS 证书验证",
                "请确认用途；表单提供恢复证书验证操作，不会自动关闭验证来绕过连接问题。",
            );
        }
    }
    for (index, rule) in value
        .pointer("/routing/rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        for field in ["domain", "ip"] {
            if let Some(values) = rule.get(field).and_then(Value::as_array)
                && (values.is_empty()
                    || values
                        .iter()
                        .any(|v| v.as_str().is_none_or(|s| s.trim().is_empty())))
            {
                advice(
                    format!("/routing/rules/{index}/{field}"),
                    "分流匹配项为空或类型需要核对",
                    "补充匹配网站 / IP，删除空行；原生组合条件和特殊用途以 Core 校验为准。",
                );
            }
        }
    }
    issues.extend(geo_and_routing_advice(value));
    issues
}

fn inbound_tcp_udp(inbound: &Value) -> (bool, bool) {
    let protocol = inbound
        .get("protocol")
        .and_then(Value::as_str)
        .unwrap_or("");
    if protocol == "dokodemo-door" {
        let net = inbound
            .pointer("/settings/network")
            .and_then(Value::as_str)
            .unwrap_or("tcp");
        let tcp = net
            .split(',')
            .any(|part| part.trim().eq_ignore_ascii_case("tcp"));
        let udp = net
            .split(',')
            .any(|part| part.trim().eq_ignore_ascii_case("udp"));
        return (tcp, udp);
    }
    if matches!(protocol, "socks" | "mixed") {
        let udp = inbound
            .pointer("/settings/udp")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return (true, udp);
    }
    (true, false)
}

fn listen_overlaps(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            "0.0.0.0".to_owned()
        } else if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
            ip.to_string()
        } else {
            trimmed.to_ascii_lowercase()
        }
    };
    let a = norm(a);
    let b = norm(b);
    let is_wild = |ip: &str| ip == "0.0.0.0" || ip == "::";
    a == b || is_wild(&a) || is_wild(&b)
}

/// Detect overlapping `(listen, port, tcp/udp)` listeners across `inbounds[]`.
pub fn inbound_port_conflicts(inbounds: &[Value]) -> Vec<(usize, String)> {
    let mut conflicts = Vec::new();
    let parsed: Vec<(usize, &str, u16, bool, bool, String)> = inbounds
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let port = item
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
                .filter(|p| *p > 0)?;
            let listen = item
                .get("listen")
                .and_then(Value::as_str)
                .unwrap_or("0.0.0.0");
            let (tcp, udp) = inbound_tcp_udp(item);
            let label = item
                .get("tag")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("入口 {}", index + 1));
            Some((index, listen, port, tcp, udp, label))
        })
        .collect();

    for (pos, (idx_a, listen_a, port_a, tcp_a, udp_a, _)) in parsed.iter().enumerate() {
        for (idx_b, listen_b, port_b, tcp_b, udp_b, label_b) in &parsed[..pos] {
            if port_a == port_b
                && listen_overlaps(listen_a, listen_b)
                && ((*tcp_a && *tcp_b) || (*udp_a && *udp_b))
            {
                let _ = idx_b;
                conflicts.push((
                    *idx_a,
                    format!("端口 {port_a} 与「{label_b}」({listen_b}:{port_b}) 监听冲突"),
                ));
                break;
            }
        }
    }
    conflicts
}

fn geo_and_routing_advice(value: &Value) -> Vec<ConfigIssue> {
    let mut issues = Vec::new();
    let mut advice = |path: String, message: &str, help| {
        issues.push(ConfigIssue {
            path,
            message: message.into(),
            help,
            blocking: false,
            code: "CONFIG_REVIEW",
        });
    };

    let mut uses_ip_rule = false;
    let mut uses_geosite = false;
    for rule in value
        .pointer("/routing/rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if rule
            .get("ip")
            .and_then(Value::as_array)
            .is_some_and(|arr| !arr.is_empty())
        {
            uses_ip_rule = true;
        }
        if rule
            .get("domain")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|s| {
                let lower = s.trim().to_ascii_lowercase();
                lower.starts_with("geosite:") || lower.starts_with("ext:")
            })
        {
            uses_geosite = true;
        }
    }

    let domain_strategy = value
        .pointer("/routing/domainStrategy")
        .and_then(Value::as_str)
        .unwrap_or("AsIs");
    if uses_ip_rule && domain_strategy == "AsIs" {
        advice(
            "/routing/domainStrategy".into(),
            "路由包含 IP / GeoIP 规则，但 domainStrategy 为 AsIs",
            "未命中域名规则的纯域名请求不会解析为 IP，会导致 geoip:cn 等 IP 规则无法命中；建议设为 IPIfNonMatch（或 IPOnDemand）。",
        );
    }

    let inbounds = value
        .get("inbounds")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let proxy_inbounds: Vec<&Value> = inbounds
        .iter()
        .filter(|inbound| inbound.get("protocol").and_then(Value::as_str) != Some("dokodemo-door"))
        .collect();
    let any_sniffing = proxy_inbounds.iter().any(|inbound| {
        inbound
            .pointer("/sniffing/enabled")
            .and_then(Value::as_bool)
            == Some(true)
    });
    if uses_geosite && !proxy_inbounds.is_empty() && !any_sniffing {
        advice(
            "/inbounds".into(),
            "路由包含 GeoSite 域名规则，但入站未启用流量嗅探 (sniffing)",
            "建议在入站启用 sniffing（destOverride 含 http/tls/quic，并设置 routeOnly: true），使纯 IP 流量也能还原域名并命中 geosite 规则。",
        );
    }
    for (index, inbound) in inbounds.iter().enumerate() {
        if inbound
            .pointer("/sniffing/enabled")
            .and_then(Value::as_bool)
            == Some(true)
            && inbound
                .pointer("/sniffing/routeOnly")
                .and_then(Value::as_bool)
                != Some(true)
        {
            advice(
                format!("/inbounds/{index}/sniffing/routeOnly"),
                "入站启用了 sniffing，但未设置 routeOnly: true",
                "建议设置 routeOnly: true，仅将嗅探域名用于路由匹配而不改写原始连接目标。",
            );
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_advice_refreshes_without_echoing_values_or_rejecting_native_documents() {
        let mut value = serde_json::json!({"outbounds":[{"protocol":"vless", "streamSettings":{"security":"reality", "realitySettings":{"shortId":"secret-invalid-value"}}}],"routing":{"rules":[{"type":"field","domain":[],"outboundTag":"missing"}]}});
        let issues = inspect_config(&value);
        for path in [
            "/outbounds/0/streamSettings/realitySettings/publicKey",
            "/outbounds/0/streamSettings/realitySettings/fingerprint",
            "/outbounds/0/streamSettings/realitySettings/shortId",
            "/routing/rules/0/domain",
        ] {
            assert!(issues.iter().any(|i| i.path == path), "{path}");
        }
        assert!(
            issues
                .iter()
                .all(|i| !i.message.contains("secret-invalid-value"))
        );
        value["outbounds"][0]["streamSettings"]["realitySettings"] = serde_json::json!({"serverName":"example.com","fingerprint":"chrome","password":"provider-public-key","shortId":"abcd"});
        value["routing"]["rules"][0]["domain"] = serde_json::json!(["domain:example.com"]);
        let repaired = inspect_config(&value);
        assert!(
            !repaired
                .iter()
                .any(|i| i.path.contains("realitySettings") || i.path.ends_with("/domain"))
        );
    }

    #[test]
    fn api_and_reverse_portals_are_native_outbound_targets() {
        let json = br#"{"api":{"tag":"api","services":["StatsService"]},"reverse":{"portals":[{"tag":"portal","domain":"reverse.invalid"}]},"outbounds":[{"tag":"direct","protocol":"freedom"}],"routing":{"rules":[{"outboundTag":"api"},{"outboundTag":"portal"},{"outboundTag":"direct"}]}}"#;
        assert!(crate::parse_config(json).is_ok());
        let mut value = parse_document(json).unwrap();
        value.as_object_mut().unwrap().remove("api");
        let issues = inspect_config(&value);
        assert!(
            issues
                .iter()
                .any(|i| i.blocking && i.path == "/routing/rules/0/outboundTag")
        );
    }

    #[test]
    fn incomplete_import_is_preserved_but_cannot_pass_runtime_checks() {
        let json = "{\n \"outbounds\": [{\"tag\":\"node\",\"custom\":{\"keep\":true}}], \"routing\":{\"rules\":[{\"outboundTag\":\"missing\"}]}\n}";
        let root = tempfile::tempdir().unwrap();
        let store = crate::Store::open(root.path()).unwrap();
        assert_eq!(store.create("incomplete", json).unwrap().json, json);
        let issues = inspect_config(&parse_document(json.as_bytes()).unwrap());
        assert!(
            issues
                .iter()
                .any(|i| i.path == "/outbounds/0/protocol" && i.blocking)
        );
        assert!(
            issues
                .iter()
                .any(|i| i.path == "/routing/rules/0/outboundTag" && i.blocking)
        );
        assert!(crate::parse_config(json.as_bytes()).is_err());
        assert!(parse_document(b"{\"outbounds\":").is_err());
        assert!(parse_document(b"[]").is_err());
    }

    #[test]
    fn optional_sections_are_advice_not_invented_required_fields() {
        let value = crate::parse_config(b"{}").unwrap();
        assert!(inspect_config(&value).iter().all(|i| !i.blocking));
        assert!(crate::parse_config(br#"{"inbounds":[{"protocol":"tun"}]}"#).is_err());
    }

    #[test]
    fn missing_node_parameters_have_field_advice_without_echoing_credentials() {
        let value = parse_document(br#"{"outbounds":[{"protocol":"vless","settings":{"vnext":[{"address":"example.invalid","users":[{}]}]}},{"protocol":"trojan","settings":{"servers":[{"port":443,"password":"test-secret-do-not-copy"}]}}]}"#).unwrap();
        let issues = inspect_config(&value);
        for path in [
            "/outbounds/0/settings/vnext/0/port",
            "/outbounds/0/settings/vnext/0/users/0/id",
            "/outbounds/1/settings/servers/0/address",
        ] {
            assert!(
                issues.iter().any(|i| i.path == path),
                "Missing field: {path}"
            );
        }
        assert!(
            issues
                .iter()
                .all(|i| !i.message.contains("test-secret-do-not-copy")
                    && !i.help.contains("test-secret-do-not-copy"))
        );
        assert!(crate::parse_config(br#"{"inbounds":"broken"}"#).is_err());
        assert!(crate::parse_config(br#"{"outbounds":[{"tag":"same","protocol":"freedom"},{"tag":"same","protocol":"freedom"}]}"#).is_err());
    }

    #[test]
    fn incomplete_socks_and_http_accounts_have_nonblocking_field_advice() {
        let value = parse_document(br#"{"outbounds":[
            {"protocol":"socks","settings":{"servers":[{"address":"proxy.invalid","port":1080,"users":[{}]}]}},
            {"protocol":"http","settings":{"servers":[{"address":"proxy.invalid","port":8080,"users":[{"user":"name"}]}]}},
            {"protocol":"socks","settings":{"servers":[{"address":"proxy.invalid","port":1081,"users":{}}]}}
        ]}"#).unwrap();
        let issues = inspect_config(&value);
        for path in [
            "/outbounds/0/settings/servers/0/users/0/user",
            "/outbounds/0/settings/servers/0/users/0/pass",
            "/outbounds/1/settings/servers/0/users/0/pass",
            "/outbounds/2/settings/servers/0/users",
        ] {
            assert!(issues.iter().any(|issue| issue.path == path), "{path}");
        }
        assert!(issues.iter().all(|issue| !issue.blocking));
    }

    #[test]
    fn geo_and_routing_advice_checks_domain_strategy_and_sniffing() {
        let value = parse_document(
            br#"{
                "inbounds":[{"tag":"socks-in","protocol":"socks","listen":"127.0.0.1","port":10808}],
                "outbounds":[{"tag":"direct","protocol":"freedom"}],
                "routing":{"rules":[
                    {"type":"field","domain":["geosite:cn"],"outboundTag":"direct"},
                    {"type":"field","ip":["geoip:cn"],"outboundTag":"direct"}
                ]}
            }"#,
        )
        .unwrap();
        let issues = inspect_config(&value);
        assert!(issues.iter().all(|i| !i.blocking));
        assert!(issues.iter().any(|i| i.path == "/routing/domainStrategy"));
        assert!(issues.iter().any(|i| i.path == "/inbounds"));

        let fixed = parse_document(
            br#"{
                "inbounds":[{"tag":"socks-in","protocol":"socks","listen":"127.0.0.1","port":10808,"sniffing":{"enabled":true,"destOverride":["http","tls","quic"],"routeOnly":true}}],
                "outbounds":[{"tag":"direct","protocol":"freedom"}],
                "routing":{"domainStrategy":"IPIfNonMatch","rules":[
                    {"type":"field","domain":["geosite:cn"],"outboundTag":"direct"},
                    {"type":"field","ip":["geoip:cn"],"outboundTag":"direct"}
                ]}
            }"#,
        )
        .unwrap();
        let fixed_issues = inspect_config(&fixed);
        assert!(
            !fixed_issues
                .iter()
                .any(|i| i.path == "/routing/domainStrategy"
                    || i.path == "/inbounds"
                    || i.path.contains("routeOnly")),
            "{:?}",
            fixed_issues.iter().map(|i| &i.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn inbound_port_conflicts_catch_overlapping_listeners_and_allow_distinct_udp_tcp() {
        let conflicting = parse_document(
            br#"{
                "inbounds":[
                    {"tag":"http-in","protocol":"http","listen":"127.0.0.1","port":10809},
                    {"tag":"pf-1","protocol":"dokodemo-door","listen":"0.0.0.0","port":10809,"settings":{"address":"10.0.0.5","port":3306,"network":"tcp"}}
                ],
                "outbounds":[{"tag":"direct","protocol":"freedom"}]
            }"#,
        )
        .unwrap();
        let issues = inspect_config(&conflicting);
        assert!(
            issues
                .iter()
                .any(|i| i.blocking && i.path == "/inbounds/1/port")
        );
        assert!(
            issues
                .iter()
                .any(|i| !i.blocking && i.path == "/inbounds/1/listen")
        );

        let disjoint_transport = parse_document(
            br#"{
                "inbounds":[
                    {"tag":"http-in","protocol":"http","listen":"127.0.0.1","port":10809},
                    {"tag":"pf-udp","protocol":"dokodemo-door","listen":"127.0.0.1","port":10809,"settings":{"address":"10.0.0.5","port":9987,"network":"udp"}}
                ],
                "outbounds":[{"tag":"direct","protocol":"freedom"}]
            }"#,
        )
        .unwrap();
        let issues = inspect_config(&disjoint_transport);
        assert!(!issues.iter().any(|i| i.blocking));
    }
}
