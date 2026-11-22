//! Guided port-forwarding (`dokodemo-door`) model, JSON sync, and validation helpers.
use crate::workspace::{PausedPortForward, Workspace};
use proxy_model::{DEFAULT_SSH_PORT, SshAuthKind, SshForward, SshHost};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::net::IpAddr;

pub const LISTEN_LOCAL: &str = "127.0.0.1";
pub const LISTEN_LAN: &str = "0.0.0.0";

pub const COMMON_TARGET_PORTS: &[(u16, &str, &str)] = &[
    (22, "SSH", "22 SSH"),
    (3306, "MySQL", "3306 MySQL"),
    (5432, "PG", "5432 PG"),
    (6379, "Redis", "6379 Redis"),
    (8080, "Web", "8080 Web"),
    (3389, "RDP", "3389 RDP"),
];

pub fn auto_ssh_forward_name(target_host: &str, target_port: u16) -> String {
    let host = {
        let trimmed = target_host.trim();
        if trimmed.is_empty() {
            LISTEN_LOCAL
        } else {
            trimmed
        }
    };
    let svc = COMMON_TARGET_PORTS
        .iter()
        .find(|(p, _, _)| *p == target_port)
        .map(|(_, label, _)| *label);
    match (svc, target_port) {
        (Some(label), port) if port > 0 => format!("{label} ({host}:{port})"),
        (_, port) if port > 0 => format!("{host}:{port}"),
        _ => host.to_owned(),
    }
}

pub fn auto_ssh_credential_name(username: &str, auth_kind: SshAuthKind) -> String {
    let user = {
        let trimmed = username.trim();
        if trimmed.is_empty() { "user" } else { trimmed }
    };
    let kind = match auth_kind {
        SshAuthKind::PrivateKey => "私钥",
        SshAuthKind::Password => "密码",
    };
    format!("{user} · {kind}")
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedSshJumpHint {
    pub user: String,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSshLCommand {
    pub listen: String,
    pub local_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub user: Option<String>,
    pub jump_host: Option<String>,
    pub ssh_port: u16,
}

const OTHER_MATCH_KEYS: &[&str] = &[
    "domain",
    "ip",
    "port",
    "sourcePort",
    "network",
    "source",
    "user",
    "protocol",
    "attrs",
    "balancerTag",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortForwardEntry {
    pub tag: String,
    pub name: String,
    pub enabled: bool,
    pub paused_index: Option<usize>,
    pub listen: String,
    pub port: u16,
    pub network: String,
    pub target_host: String,
    pub target_port: u16,
    pub outbound_tag: String,
    pub custom_route: bool,
    pub missing_route: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PortForwardForm {
    pub editing_tag: Option<String>,
    pub editing_paused_index: Option<usize>,
    pub editing_ssh_id: Option<String>,
    pub is_clone: bool,
    pub start_now: bool,
    pub ssh_cmd_input: String,
    pub ssh_cmd_feedback: Option<(bool, String)>,
    pub parsed_ssh_hint: Option<ParsedSshJumpHint>,
    pub name: String,
    pub listen: String,
    pub custom_listen: bool,
    pub local_port: String,
    pub network: String,
    pub target_host: String,
    pub target_port: String,
    pub outbound_is_ssh: bool,
    pub outbound_tag: String,
    pub ssh_host_id: String,
}

impl PortForwardForm {
    #[cfg(test)]
    pub fn from_new(config: &Value, paused: &[PausedPortForward]) -> Self {
        Self::from_new_with_ssh(config, paused, &[])
    }

    pub fn from_new_with_ssh(
        config: &Value,
        paused: &[PausedPortForward],
        ssh_forwards: &[SshForward],
    ) -> Self {
        let default_outbound = config
            .pointer("/outbounds/0/tag")
            .and_then(Value::as_str)
            .unwrap_or("direct")
            .to_owned();
        let port = next_free_port_all(10820, Some(config), paused, ssh_forwards, None, None);
        Self {
            editing_tag: None,
            editing_paused_index: None,
            editing_ssh_id: None,
            is_clone: false,
            start_now: true,
            ssh_cmd_input: String::new(),
            ssh_cmd_feedback: None,
            parsed_ssh_hint: None,
            name: String::new(),
            listen: LISTEN_LOCAL.into(),
            custom_listen: false,
            local_port: port.to_string(),
            network: "tcp".into(),
            target_host: String::new(),
            target_port: String::new(),
            outbound_is_ssh: false,
            outbound_tag: default_outbound,
            ssh_host_id: String::new(),
        }
    }

    pub fn from_new_ssh(
        config: Option<&Value>,
        paused: &[PausedPortForward],
        ssh_forwards: &[SshForward],
        preselect_host_id: Option<&str>,
    ) -> Self {
        let default_outbound = config
            .and_then(|c| c.pointer("/outbounds/0/tag"))
            .and_then(Value::as_str)
            .unwrap_or("direct")
            .to_owned();
        let port = next_free_port_all(15432, config, paused, ssh_forwards, None, None);
        Self {
            editing_tag: None,
            editing_paused_index: None,
            editing_ssh_id: None,
            is_clone: false,
            start_now: true,
            ssh_cmd_input: String::new(),
            ssh_cmd_feedback: None,
            parsed_ssh_hint: None,
            name: String::new(),
            listen: LISTEN_LOCAL.into(),
            custom_listen: false,
            local_port: port.to_string(),
            network: "tcp".into(),
            target_host: "127.0.0.1".into(),
            target_port: String::new(),
            outbound_is_ssh: true,
            outbound_tag: default_outbound,
            ssh_host_id: preselect_host_id.unwrap_or_default().to_owned(),
        }
    }

    pub fn from_entry(entry: &PortForwardEntry) -> Self {
        let custom_listen =
            entry.listen.trim() != LISTEN_LOCAL && entry.listen.trim() != LISTEN_LAN;
        Self {
            editing_tag: entry.enabled.then(|| entry.tag.clone()),
            editing_paused_index: entry.paused_index,
            editing_ssh_id: None,
            is_clone: false,
            start_now: true,
            ssh_cmd_input: String::new(),
            ssh_cmd_feedback: None,
            parsed_ssh_hint: None,
            name: entry.name.clone(),
            listen: entry.listen.clone(),
            custom_listen,
            local_port: entry.port.to_string(),
            network: entry.network.clone(),
            target_host: entry.target_host.clone(),
            target_port: if entry.target_port > 0 {
                entry.target_port.to_string()
            } else {
                String::new()
            },
            outbound_is_ssh: false,
            outbound_tag: entry.outbound_tag.clone(),
            ssh_host_id: String::new(),
        }
    }

    pub fn from_ssh_forward_with_running(fwd: &SshForward, running: bool) -> Self {
        let custom_listen = fwd.listen.trim() != LISTEN_LOCAL && fwd.listen.trim() != LISTEN_LAN;
        Self {
            editing_tag: None,
            editing_paused_index: None,
            editing_ssh_id: Some(fwd.id.clone()),
            is_clone: false,
            start_now: running,
            ssh_cmd_input: String::new(),
            ssh_cmd_feedback: None,
            parsed_ssh_hint: None,
            name: fwd.name.clone(),
            listen: fwd.listen.clone(),
            custom_listen,
            local_port: fwd.port.to_string(),
            network: "tcp".into(),
            target_host: fwd.target_host.clone(),
            target_port: fwd.target_port.to_string(),
            outbound_is_ssh: true,
            outbound_tag: String::new(),
            ssh_host_id: fwd.ssh_host_id.clone(),
        }
    }

    pub fn from_clone_ssh(
        fwd: &SshForward,
        config: Option<&Value>,
        paused: &[PausedPortForward],
        ssh_forwards: &[SshForward],
    ) -> Self {
        let custom_listen = fwd.listen.trim() != LISTEN_LOCAL && fwd.listen.trim() != LISTEN_LAN;
        let next_port = next_free_port_all(
            fwd.port.saturating_add(1),
            config,
            paused,
            ssh_forwards,
            None,
            None,
        );
        let cloned_name = if fwd.name.trim().is_empty() {
            String::new()
        } else {
            format!("{} - 副本", fwd.name.trim())
        };
        Self {
            editing_tag: None,
            editing_paused_index: None,
            editing_ssh_id: None,
            is_clone: true,
            start_now: true,
            ssh_cmd_input: String::new(),
            ssh_cmd_feedback: None,
            parsed_ssh_hint: None,
            name: cloned_name,
            listen: fwd.listen.clone(),
            custom_listen,
            local_port: next_port.to_string(),
            network: "tcp".into(),
            target_host: fwd.target_host.clone(),
            target_port: fwd.target_port.to_string(),
            outbound_is_ssh: true,
            outbound_tag: String::new(),
            ssh_host_id: fwd.ssh_host_id.clone(),
        }
    }
}

/// Normalizes a `dokodemo-door` `settings.network` value to `"tcp"`, `"udp"`, or `"tcp,udp"`.
pub fn normalize_network(raw: Option<&str>) -> String {
    let raw = raw.unwrap_or("tcp");
    let has_tcp = raw.split(',').any(|p| p.trim().eq_ignore_ascii_case("tcp"));
    let has_udp = raw.split(',').any(|p| p.trim().eq_ignore_ascii_case("udp"));
    match (has_tcp, has_udp) {
        (true, true) => "tcp,udp".into(),
        (false, true) => "udp".into(),
        _ => "tcp".into(),
    }
}

pub fn network_label(network: &str) -> &'static str {
    match normalize_network(Some(network)).as_str() {
        "udp" => "UDP",
        "tcp,udp" => "TCP+UDP",
        _ => "TCP",
    }
}

/// Splits pasted `[ipv6]:port` or `host:port` text into `(host, port)`.
pub fn parse_host_port_paste(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        let (host, port_part) = rest.split_once("]:")?;
        let port = port_part.parse::<u16>().ok().filter(|p| *p > 0)?;
        return (!host.trim().is_empty()).then(|| (host.trim().to_owned(), port.to_string()));
    }
    let (host, port_part) = trimmed.rsplit_once(':')?;
    // Do not split unbracketed IPv6 addresses containing multiple colons.
    if host.contains(':') || host.trim().is_empty() || host.contains(char::is_whitespace) {
        return None;
    }
    let port = port_part.parse::<u16>().ok().filter(|p| *p > 0)?;
    Some((host.trim().to_owned(), port.to_string()))
}

/// Parses pasted `[user@]host[:port]` or `[user@][ipv6][:port]` jump-host text.
/// Returns `None` when neither `@` nor a valid `:port` suffix is present.
pub fn parse_ssh_host_paste(input: &str) -> Option<(Option<String>, String, Option<u16>)> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    let (user, rest) = if let Some((u, after)) = trimmed.split_once('@') {
        let u_trim = u.trim();
        if u_trim.is_empty() || after.trim().is_empty() {
            return None;
        }
        (Some(u_trim.to_owned()), after.trim())
    } else {
        (None, trimmed)
    };

    if let Some(inner) = rest.strip_prefix('[') {
        if let Some((host, port_part)) = inner.split_once("]:") {
            let port = port_part.parse::<u16>().ok().filter(|p| *p > 0)?;
            let host = host.trim();
            if host.is_empty() {
                return None;
            }
            return Some((user, host.to_owned(), Some(port)));
        }
        if let Some(host) = inner.strip_suffix(']') {
            let host = host.trim();
            if user.is_none() || host.is_empty() {
                return None;
            }
            return Some((user, host.to_owned(), None));
        }
        return None;
    }

    let colon_count = rest.bytes().filter(|b| *b == b':').count();
    if colon_count == 1 {
        let (host, port_part) = rest.rsplit_once(':')?;
        let port = port_part.parse::<u16>().ok().filter(|p| *p > 0)?;
        let host = host.trim();
        if host.is_empty() {
            return None;
        }
        return Some((user, host.to_owned(), Some(port)));
    }
    if colon_count > 1 && user.is_some() {
        return Some((user, rest.to_owned(), None));
    }
    if colon_count == 0 && user.is_some() {
        return Some((user, rest.to_owned(), None));
    }
    None
}

fn tokenize_shell_words(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for ch in raw.chars() {
        match quote {
            Some(q) if ch == q => {
                quote = None;
            }
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
            }
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn split_l_spec_segments(spec: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for ch in spec.chars() {
        match ch {
            '[' => {
                depth += 1;
                cur.push(ch);
            }
            ']' => {
                depth = depth.saturating_sub(1);
                cur.push(ch);
            }
            ':' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    out.push(cur);
    out
}

fn strip_brackets(host: &str) -> String {
    let t = host.trim();
    t.strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(t)
        .trim()
        .to_owned()
}

pub fn parse_ssh_l_command(raw: &str) -> Result<ParsedSshLCommand, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("请先粘贴 ssh -L 命令".into());
    }
    let tokens = tokenize_shell_words(trimmed);
    let mut l_spec: Option<String> = None;
    let mut ssh_port: u16 = DEFAULT_SSH_PORT;
    let mut user: Option<String> = None;
    let mut jump_host: Option<String> = None;

    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i].as_str();
        if i == 0 && tok.eq_ignore_ascii_case("ssh") {
            i += 1;
            continue;
        }
        if tok == "-L" {
            if let Some(next) = tokens.get(i + 1) {
                l_spec = Some(next.clone());
                i += 2;
                continue;
            }
        } else if let Some(rest) = tok.strip_prefix("-L")
            && !rest.is_empty()
        {
            l_spec = Some(rest.to_owned());
            i += 1;
            continue;
        } else if tok == "-p" {
            if let Some(next) = tokens.get(i + 1) {
                if let Ok(p) = next.parse::<u16>()
                    && p > 0
                {
                    ssh_port = p;
                }
                i += 2;
                continue;
            }
        } else if let Some(rest) = tok.strip_prefix("-p")
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_digit())
        {
            if let Ok(p) = rest.parse::<u16>()
                && p > 0
            {
                ssh_port = p;
            }
            i += 1;
            continue;
        } else if tok == "-l" {
            if let Some(next) = tokens.get(i + 1)
                && !next.trim().is_empty()
            {
                user = Some(next.trim().to_owned());
                i += 2;
                continue;
            }
        } else if matches!(
            tok,
            "-i" | "-F"
                | "-o"
                | "-J"
                | "-W"
                | "-b"
                | "-c"
                | "-D"
                | "-R"
                | "-E"
                | "-I"
                | "-m"
                | "-S"
                | "-w"
        ) {
            i += 2;
            continue;
        } else if tok.starts_with('-') {
            i += 1;
            continue;
        } else if let Some((u, h)) = tok.split_once('@') {
            if !u.trim().is_empty() {
                user = Some(u.trim().to_owned());
            }
            if !h.trim().is_empty() {
                jump_host = Some(strip_brackets(h));
            }
        } else if jump_host.is_none() && !tok.contains(':') {
            jump_host = Some(strip_brackets(tok));
        }
        i += 1;
    }

    let Some(spec) = l_spec else {
        return Err("未检测到 -L 本地转发参数（格式示例：-L 15432:10.0.1.8:5432）".into());
    };
    let segs = split_l_spec_segments(&spec);
    let (listen, lport_str, thost_raw, tport_str) = match segs.as_slice() {
        [lp, th, tp] => (
            LISTEN_LOCAL.to_owned(),
            lp.as_str(),
            th.as_str(),
            tp.as_str(),
        ),
        [bind, lp, th, tp] => {
            let clean_bind = strip_brackets(bind);
            let listen = if clean_bind.is_empty() || clean_bind.eq_ignore_ascii_case("localhost") {
                LISTEN_LOCAL.to_owned()
            } else {
                clean_bind
            };
            (listen, lp.as_str(), th.as_str(), tp.as_str())
        }
        _ => {
            return Err("-L 参数段数异常，应为 [绑定IP:]本地端口:目标主机:目标端口".into());
        }
    };

    let Some(local_port) = lport_str.trim().parse::<u16>().ok().filter(|p| *p > 0) else {
        return Err("-L 中的本地端口无效（应为 1–65535）".into());
    };
    let target_host = strip_brackets(thost_raw);
    if target_host.is_empty() || target_host.chars().any(char::is_whitespace) {
        return Err("-L 中的目标地址不能为空".into());
    }
    let Some(target_port) = tport_str.trim().parse::<u16>().ok().filter(|p| *p > 0) else {
        return Err("-L 中的目标端口无效（应为 1–65535）".into());
    };

    Ok(ParsedSshLCommand {
        listen,
        local_port,
        target_host,
        target_port,
        user,
        jump_host,
        ssh_port,
    })
}

pub fn match_ssh_host_id(hosts: &[SshHost], jump_host: &str, ssh_port: u16) -> Option<String> {
    let target = jump_host.trim();
    if target.is_empty() {
        return None;
    }
    hosts
        .iter()
        .find(|h| h.host.eq_ignore_ascii_case(target) && h.port == ssh_port)
        .or_else(|| hosts.iter().find(|h| h.host.eq_ignore_ascii_case(target)))
        .map(|h| h.id.clone())
}

pub fn is_loopback_listen(listen: &str) -> bool {
    listen
        .trim()
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

pub fn is_private_or_loopback_host(host: &str) -> bool {
    let trimmed = host.trim();
    if trimmed.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match trimmed.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        Ok(IpAddr::V6(v6)) => v6.is_loopback() || v6.is_unique_local(),
        Err(_) => false,
    }
}

pub fn is_managed_rule_for_tag(rule: &Value, tag: &str) -> bool {
    let Some(obj) = rule.as_object() else {
        return false;
    };
    if let Some(kind) = obj.get("type").and_then(Value::as_str)
        && kind != "field"
    {
        return false;
    }
    if obj
        .get("outboundTag")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return false;
    }
    if OTHER_MATCH_KEYS.iter().any(|key| obj.contains_key(*key)) {
        return false;
    }
    let Some(tags) = obj.get("inboundTag").and_then(Value::as_array) else {
        return false;
    };
    tags.len() == 1 && tags.first().and_then(Value::as_str) == Some(tag)
}

fn rule_references_tag(rule: &Value, tag: &str) -> bool {
    rule.get("inboundTag")
        .and_then(Value::as_array)
        .is_some_and(|tags| tags.iter().any(|t| t.as_str() == Some(tag)))
}

fn api_tag(config: &Value) -> Option<&str> {
    config
        .pointer("/api/tag")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
}

/// Returns non-API `dokodemo-door` inbound tags declared in `config`.
pub fn dokodemo_tags(config: &Value) -> HashSet<String> {
    let api = api_tag(config);
    config
        .get("inbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|inbound| inbound.get("protocol").and_then(Value::as_str) == Some("dokodemo-door"))
        .filter_map(|inbound| inbound.get("tag").and_then(Value::as_str))
        .filter(|tag| !tag.is_empty() && api != Some(*tag))
        .map(str::to_owned)
        .collect()
}

pub fn available_outbounds(
    config: &Value,
    names: &BTreeMap<String, String>,
) -> Vec<(String, String)> {
    config
        .get("outbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| {
            let tag = node.get("tag").and_then(Value::as_str)?.to_owned();
            if tag.is_empty() {
                return None;
            }
            let display = names
                .get(&tag)
                .filter(|s| !s.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| tag.clone());
            let protocol = node
                .get("protocol")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Some((tag.clone(), format!("{display} ({tag} · {protocol})")))
        })
        .collect()
}

pub fn extract_entries(
    config: &Value,
    workspace: &Workspace,
    profile_id: &str,
) -> Vec<PortForwardEntry> {
    let api = api_tag(config);
    let default_outbound = config
        .pointer("/outbounds/0/tag")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let rules = config
        .pointer("/routing/rules")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let pf_names = workspace.port_forward_names.get(profile_id);
    let mut entries = Vec::new();

    for (index, inbound) in config
        .get("inbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if inbound.get("protocol").and_then(Value::as_str) != Some("dokodemo-door") {
            continue;
        }
        let tag = inbound
            .get("tag")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("inbound-{}", index + 1));
        if api == Some(tag.as_str()) {
            continue;
        }
        let listen = inbound
            .get("listen")
            .and_then(Value::as_str)
            .unwrap_or(LISTEN_LAN)
            .to_owned();
        let port = inbound
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(0);
        let network =
            normalize_network(inbound.pointer("/settings/network").and_then(Value::as_str));
        let target_host = inbound
            .pointer("/settings/address")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let target_port = inbound
            .pointer("/settings/port")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(0);

        let matching_rules: Vec<&Value> = rules
            .iter()
            .filter(|rule| rule_references_tag(rule, &tag))
            .collect();
        let (outbound_tag, custom_route, missing_route) =
            if matching_rules.len() == 1 && is_managed_rule_for_tag(matching_rules[0], &tag) {
                (
                    matching_rules[0]
                        .get("outboundTag")
                        .and_then(Value::as_str)
                        .unwrap_or(default_outbound)
                        .to_owned(),
                    false,
                    false,
                )
            } else if matching_rules.is_empty() {
                (default_outbound.to_owned(), false, true)
            } else {
                (
                    matching_rules[0]
                        .get("outboundTag")
                        .and_then(Value::as_str)
                        .unwrap_or(default_outbound)
                        .to_owned(),
                    true,
                    false,
                )
            };

        let name = pf_names
            .and_then(|map| map.get(&tag))
            .cloned()
            .unwrap_or_default();

        entries.push(PortForwardEntry {
            tag,
            name,
            enabled: true,
            paused_index: None,
            listen,
            port,
            network,
            target_host,
            target_port,
            outbound_tag,
            custom_route,
            missing_route,
        });
    }

    if let Some(paused) = workspace.paused_port_forwards.get(profile_id) {
        for (idx, item) in paused.iter().enumerate() {
            entries.push(PortForwardEntry {
                tag: item.tag.clone(),
                name: item.name.clone(),
                enabled: false,
                paused_index: Some(idx),
                listen: item.listen.clone(),
                port: item.port,
                network: normalize_network(Some(&item.network)),
                target_host: item.target_host.clone(),
                target_port: item.target_port,
                outbound_tag: item.outbound_tag.clone(),
                custom_route: false,
                missing_route: false,
            });
        }
    }

    entries
}

pub fn next_free_port(config: &Value, paused: &[PausedPortForward]) -> u16 {
    next_free_port_all(10820, Some(config), paused, &[], None, None)
}

pub fn next_free_port_all(
    preferred_start: u16,
    config: Option<&Value>,
    paused: &[PausedPortForward],
    ssh_forwards: &[SshForward],
    exclude_tag: Option<&str>,
    exclude_ssh_id: Option<&str>,
) -> u16 {
    let mut used: HashSet<u16> = config
        .and_then(|c| c.get("inbounds"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|i| exclude_tag.is_none_or(|ex| i.get("tag").and_then(Value::as_str) != Some(ex)))
        .filter_map(|i| i.get("port").and_then(Value::as_u64))
        .filter_map(|p| u16::try_from(p).ok())
        .collect();
    used.extend(paused.iter().map(|p| p.port));
    used.extend(
        ssh_forwards
            .iter()
            .filter(|f| exclude_ssh_id.is_none_or(|ex| f.id != ex))
            .map(|f| f.port),
    );
    let start = preferred_start.max(1);
    (start..=u16::MAX)
        .chain(10820..start)
        .find(|port| !used.contains(port))
        .unwrap_or(start)
}

pub fn next_free_tag(config: &Value, paused: &[PausedPortForward]) -> String {
    let mut used: HashSet<String> = config
        .get("inbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|i| i.get("tag").and_then(Value::as_str).map(str::to_owned))
        .collect();
    for rule in config
        .pointer("/routing/rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(tags) = rule.get("inboundTag").and_then(Value::as_array) {
            used.extend(tags.iter().filter_map(Value::as_str).map(str::to_owned));
        }
    }
    used.extend(paused.iter().map(|p| p.tag.clone()));
    (1..)
        .map(|n| format!("pf-{n}"))
        .find(|tag| !used.contains(tag))
        .unwrap()
}

fn form_to_inbound(tag: &str, form: &PortForwardForm, base: Option<&Value>) -> Value {
    let port = form.local_port.trim().parse::<u16>().unwrap_or(10820);
    let target_port = form.target_port.trim().parse::<u16>().unwrap_or(0);
    let network = normalize_network(Some(&form.network));
    let mut inbound = base.and_then(Value::as_object).cloned().unwrap_or_default();
    inbound.insert("tag".into(), json!(tag));
    inbound.insert("listen".into(), json!(form.listen.trim()));
    inbound.insert("port".into(), json!(port));
    inbound.insert("protocol".into(), json!("dokodemo-door"));
    let mut settings = inbound
        .get("settings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    settings.insert("address".into(), json!(form.target_host.trim()));
    settings.insert("port".into(), json!(target_port));
    settings.insert("network".into(), json!(network));
    settings
        .entry("followRedirect")
        .or_insert_with(|| json!(false));
    inbound.insert("settings".into(), Value::Object(settings));
    Value::Object(inbound)
}

pub fn check_form_error(config: &Value, form: &PortForwardForm) -> Option<String> {
    check_form_error_with_ssh(config, form, &[], &[])
}

pub fn check_form_error_with_ssh(
    config: &Value,
    form: &PortForwardForm,
    ssh_hosts: &[SshHost],
    ssh_forwards: &[SshForward],
) -> Option<String> {
    check_form_error_with_optional_config(Some(config), form, ssh_hosts, ssh_forwards)
}

pub fn find_local_port_conflict(
    config: Option<&Value>,
    form: &PortForwardForm,
    ssh_forwards: &[SshForward],
) -> Option<String> {
    let listen = form.listen.trim();
    if listen.parse::<IpAddr>().is_err() {
        return None;
    }
    let local_port = form
        .local_port
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)?;

    if let Some(config) = config {
        let mut simulated = config
            .get("inbounds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let candidate_tag = form
            .editing_tag
            .as_deref()
            .unwrap_or("__pf-preview-candidate__");
        if let Some(existing_tag) = form.editing_tag.as_deref() {
            simulated.retain(|item| item.get("tag").and_then(Value::as_str) != Some(existing_tag));
        }
        let candidate_inbound = json!({
            "tag": candidate_tag,
            "listen": listen,
            "port": local_port,
            "protocol": "dokodemo-door",
            "settings": {"network": if form.outbound_is_ssh { "tcp".to_owned() } else { normalize_network(Some(&form.network)) }}
        });
        let candidate_idx = simulated.len();
        simulated.push(candidate_inbound);
        if let Some((_, msg)) = proxy_config::inbound_port_conflicts(&simulated)
            .into_iter()
            .find(|(idx, _)| *idx == candidate_idx)
        {
            return Some(msg);
        }
    }

    if let Some(other) = ssh_forwards.iter().find(|f| {
        form.editing_ssh_id.as_deref() != Some(f.id.as_str())
            && f.port == local_port
            && listens_overlap(&f.listen, listen)
    }) {
        return Some(format!(
            "端口 {local_port} 与 SSH 隧道「{}」冲突。",
            other.name
        ));
    }
    None
}

pub fn check_form_error_with_optional_config(
    config: Option<&Value>,
    form: &PortForwardForm,
    ssh_hosts: &[SshHost],
    ssh_forwards: &[SshForward],
) -> Option<String> {
    if !form.outbound_is_ssh && config.is_none() {
        return Some("当前 Xray 配置 JSON 存在语法错误，请先修正 JSON 或切换为 SSH 隧道。".into());
    }
    let listen = form.listen.trim();
    if listen.parse::<IpAddr>().is_err() {
        return Some("监听 IP 格式无效（例如 127.0.0.1 或 0.0.0.0）。".into());
    }
    if form
        .local_port
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .is_none()
    {
        return Some("本地端口应为 1–65535 的整数。".into());
    }
    if form.target_host.trim().is_empty() {
        return Some("请填写目标域名或 IP。".into());
    }
    if form
        .target_port
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .is_none()
    {
        return Some("目标端口应为 1–65535 的整数。".into());
    }
    if form.outbound_is_ssh {
        if normalize_network(Some(&form.network)) != "tcp" {
            return Some("SSH 隧道仅支持 TCP 端口转发，请切换为 TCP。".into());
        }
        if !ssh_hosts.iter().any(|h| h.id == form.ssh_host_id) {
            return Some("请选择或新建一个 SSH 跳板机。".into());
        }
    } else if let Some(cfg) = config {
        let valid_outbound =
            cfg.get("outbounds")
                .and_then(Value::as_array)
                .is_some_and(|outbounds| {
                    outbounds.iter().any(|o| {
                        o.get("tag").and_then(Value::as_str) == Some(form.outbound_tag.as_str())
                    })
                });
        if !valid_outbound {
            return Some("请选择有效的转发出口节点。".into());
        }
    }
    find_local_port_conflict(config, form, ssh_forwards)
}

fn listens_overlap(a: &str, b: &str) -> bool {
    let Ok(ip_a) = a.trim().parse::<IpAddr>() else {
        return a.trim() == b.trim();
    };
    let Ok(ip_b) = b.trim().parse::<IpAddr>() else {
        return a.trim() == b.trim();
    };
    ip_a == ip_b || ip_a.is_unspecified() || ip_b.is_unspecified()
}

pub fn ssh_forward_from_form(form: &PortForwardForm) -> SshForward {
    let target_host = form.target_host.trim().to_owned();
    let target_port = form.target_port.trim().parse::<u16>().unwrap_or(0);
    let name = if form.name.trim().is_empty() {
        auto_ssh_forward_name(&target_host, target_port)
    } else {
        form.name.trim().to_owned()
    };
    SshForward {
        id: form.editing_ssh_id.clone().unwrap_or_default(),
        name,
        listen: form.listen.trim().to_owned(),
        port: form.local_port.trim().parse::<u16>().unwrap_or(0),
        target_host,
        target_port,
        ssh_host_id: form.ssh_host_id.clone(),
    }
}

fn ensure_top_managed_rule(
    config: &mut Value,
    tag: &str,
    outbound_tag: &str,
) -> Result<(), String> {
    let api_tag = api_tag(config).map(str::to_owned);
    if config.get("routing").is_none() {
        config["routing"] = json!({});
    }
    let Some(routing) = config.get_mut("routing").and_then(Value::as_object_mut) else {
        return Err("routing 字段不是对象，请在原始 JSON 中修正。".into());
    };
    if !routing.contains_key("rules") {
        routing.insert("rules".into(), json!([]));
    }
    let Some(rules) = routing.get_mut("rules").and_then(Value::as_array_mut) else {
        return Err("routing.rules 字段不是数组，请在原始 JSON 中修正。".into());
    };

    let managed = json!({
        "type": "field",
        "inboundTag": [tag],
        "outboundTag": outbound_tag
    });

    if let Some(pos) = rules.iter().position(|r| is_managed_rule_for_tag(r, tag)) {
        rules[pos] = managed;
        return Ok(());
    }
    // Insert after any leading internal API rule or existing dokodemo-door rules,
    // before general domain/ip rules so private target IPs are not captured by geoip:private.
    let insert_at = rules
        .iter()
        .position(|rule| {
            let is_api_rule = api_tag
                .as_deref()
                .is_some_and(|api| rule_references_tag(rule, api));
            let is_pf_rule = rule
                .get("inboundTag")
                .and_then(Value::as_array)
                .is_some_and(|tags| {
                    tags.iter()
                        .filter_map(Value::as_str)
                        .any(|t| is_managed_rule_for_tag(rule, t))
                });
            !(is_api_rule || is_pf_rule)
        })
        .unwrap_or(rules.len());
    rules.insert(insert_at, managed);
    Ok(())
}

pub fn save_entry(
    config: &mut Value,
    workspace: &mut Workspace,
    profile_id: &str,
    form: &PortForwardForm,
) -> Result<String, String> {
    if let Some(err) = check_form_error(config, form) {
        return Err(err);
    }
    let paused_slice = workspace
        .paused_port_forwards
        .get(profile_id)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    if let Some(paused_idx) = form.editing_paused_index {
        let listen = form.listen.trim().to_owned();
        let port = form.local_port.trim().parse::<u16>().unwrap();
        let target_host = form.target_host.trim().to_owned();
        let target_port = form.target_port.trim().parse::<u16>().unwrap();
        let network = normalize_network(Some(&form.network));
        let Some(list) = workspace.paused_port_forwards.get_mut(profile_id) else {
            return Err("未找到要编辑的已停用转发条目。".into());
        };
        let Some(item) = list.get_mut(paused_idx) else {
            return Err("未找到要编辑的已停用转发条目。".into());
        };
        item.name = form.name.trim().to_owned();
        item.listen = listen;
        item.port = port;
        item.network = network;
        item.target_host = target_host;
        item.target_port = target_port;
        item.outbound_tag = form.outbound_tag.clone();
        return Ok(item.tag.clone());
    }

    let tag = form
        .editing_tag
        .clone()
        .unwrap_or_else(|| next_free_tag(config, paused_slice));
    if config.get("inbounds").is_none() {
        config["inbounds"] = json!([]);
    }
    let Some(inbounds) = config.get_mut("inbounds").and_then(Value::as_array_mut) else {
        return Err("inbounds 字段不是数组，请在原始 JSON 中修正。".into());
    };
    if let Some(existing) = inbounds
        .iter_mut()
        .find(|i| i.get("tag").and_then(Value::as_str) == Some(tag.as_str()))
    {
        let updated = form_to_inbound(&tag, form, Some(existing));
        *existing = updated;
    } else {
        inbounds.push(form_to_inbound(&tag, form, None));
    }

    let has_complex_route = config
        .pointer("/routing/rules")
        .and_then(Value::as_array)
        .is_some_and(|rules| {
            let matching: Vec<&Value> = rules
                .iter()
                .filter(|r| rule_references_tag(r, &tag))
                .collect();
            matching.len() > 1
                || (matching.len() == 1 && !is_managed_rule_for_tag(matching[0], &tag))
        });
    if !has_complex_route {
        ensure_top_managed_rule(config, &tag, &form.outbound_tag)?;
    }

    let trimmed_name = form.name.trim();
    if trimmed_name.is_empty() {
        if let Some(names) = workspace.port_forward_names.get_mut(profile_id) {
            names.remove(&tag);
        }
    } else {
        workspace
            .port_forward_names
            .entry(profile_id.to_owned())
            .or_default()
            .insert(tag.clone(), trimmed_name.to_owned());
    }
    Ok(tag)
}

pub fn set_entry_enabled(
    config: &mut Value,
    workspace: &mut Workspace,
    profile_id: &str,
    entry: &PortForwardEntry,
    enable: bool,
) -> Result<(), String> {
    if entry.enabled == enable {
        return Ok(());
    }
    if !enable {
        let mut extra_inbound = None;
        if let Some(inbounds) = config.get_mut("inbounds").and_then(Value::as_array_mut)
            && let Some(pos) = inbounds
                .iter()
                .position(|i| i.get("tag").and_then(Value::as_str) == Some(entry.tag.as_str()))
        {
            extra_inbound = Some(inbounds.remove(pos));
        }
        if let Some(rules) = config
            .pointer_mut("/routing/rules")
            .and_then(Value::as_array_mut)
        {
            rules.retain(|rule| !is_managed_rule_for_tag(rule, &entry.tag));
        }
        if let Some(names) = workspace.port_forward_names.get_mut(profile_id) {
            names.remove(&entry.tag);
        }
        workspace
            .paused_port_forwards
            .entry(profile_id.to_owned())
            .or_default()
            .push(PausedPortForward {
                tag: entry.tag.clone(),
                name: entry.name.clone(),
                listen: entry.listen.clone(),
                port: entry.port,
                network: entry.network.clone(),
                target_host: entry.target_host.clone(),
                target_port: entry.target_port,
                outbound_tag: entry.outbound_tag.clone(),
                extra_inbound,
            });
        return Ok(());
    }

    let Some(paused_idx) = entry.paused_index else {
        return Ok(());
    };
    let paused_item = workspace
        .paused_port_forwards
        .get(profile_id)
        .and_then(|list| list.get(paused_idx))
        .cloned()
        .ok_or_else(|| "未找到停用的端口转发条目。".to_owned())?;

    let mut form = PortForwardForm {
        editing_tag: None,
        editing_paused_index: None,
        name: paused_item.name.clone(),
        listen: paused_item.listen.clone(),
        custom_listen: false,
        local_port: paused_item.port.to_string(),
        network: paused_item.network.clone(),
        target_host: paused_item.target_host.clone(),
        target_port: paused_item.target_port.to_string(),
        outbound_tag: paused_item.outbound_tag.clone(),
        ..Default::default()
    };
    if let Some(err) = check_form_error(config, &form) {
        return Err(err);
    }

    let tag_taken = config
        .get("inbounds")
        .and_then(Value::as_array)
        .is_some_and(|inbounds| {
            inbounds
                .iter()
                .any(|i| i.get("tag").and_then(Value::as_str) == Some(paused_item.tag.as_str()))
        });
    let tag = if tag_taken {
        let empty = &[];
        let list = workspace
            .paused_port_forwards
            .get(profile_id)
            .map(Vec::as_slice)
            .unwrap_or(empty);
        next_free_tag(config, list)
    } else {
        paused_item.tag.clone()
    };
    form.editing_tag = Some(tag.clone());
    if config.get("inbounds").is_none() {
        config["inbounds"] = json!([]);
    }
    let Some(inbounds) = config.get_mut("inbounds").and_then(Value::as_array_mut) else {
        return Err("inbounds 字段不是数组，请在原始 JSON 中修正。".into());
    };
    inbounds.push(form_to_inbound(
        &tag,
        &form,
        paused_item.extra_inbound.as_ref(),
    ));
    ensure_top_managed_rule(config, &tag, &paused_item.outbound_tag)?;

    if !paused_item.name.is_empty() {
        workspace
            .port_forward_names
            .entry(profile_id.to_owned())
            .or_default()
            .insert(tag, paused_item.name);
    }
    if let Some(list) = workspace.paused_port_forwards.get_mut(profile_id) {
        list.remove(paused_idx);
        if list.is_empty() {
            workspace.paused_port_forwards.remove(profile_id);
        }
    }
    Ok(())
}

pub fn remove_entry(
    config: &mut Value,
    workspace: &mut Workspace,
    profile_id: &str,
    entry: &PortForwardEntry,
) {
    if entry.enabled {
        if let Some(inbounds) = config.get_mut("inbounds").and_then(Value::as_array_mut) {
            inbounds.retain(|i| i.get("tag").and_then(Value::as_str) != Some(entry.tag.as_str()));
        }
        if let Some(rules) = config
            .pointer_mut("/routing/rules")
            .and_then(Value::as_array_mut)
        {
            rules.retain(|rule| !is_managed_rule_for_tag(rule, &entry.tag));
        }
        if let Some(names) = workspace.port_forward_names.get_mut(profile_id) {
            names.remove(&entry.tag);
        }
    } else if let Some(idx) = entry.paused_index
        && let Some(list) = workspace.paused_port_forwards.get_mut(profile_id)
        && idx < list.len()
    {
        list.remove(idx);
        if list.is_empty() {
            workspace.paused_port_forwards.remove(profile_id);
        }
    }
}

/// Counts `(active, paused)` port-forward references keyed by `outboundTag`.
pub fn outbound_ref_counts(
    config: &Value,
    workspace: &Workspace,
    profile_id: &str,
) -> BTreeMap<String, (usize, usize)> {
    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for entry in extract_entries(config, workspace, profile_id) {
        if entry.outbound_tag.is_empty() {
            continue;
        }
        let slot = counts.entry(entry.outbound_tag).or_default();
        if entry.enabled {
            slot.0 += 1;
        } else {
            slot.1 += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> Value {
        json!({
            "inbounds": [
                {"tag": "socks-in", "listen": "127.0.0.1", "port": 10808, "protocol": "socks"},
                {"tag": "http-in", "listen": "127.0.0.1", "port": 10809, "protocol": "http"}
            ],
            "outbounds": [
                {"tag": "proxy-jp", "protocol": "vless"},
                {"tag": "direct", "protocol": "freedom"}
            ],
            "routing": {
                "domainStrategy": "IPIfNonMatch",
                "rules": [
                    {"type": "field", "domain": ["geosite:cn"], "outboundTag": "direct"}
                ]
            }
        })
    }

    #[test]
    fn host_port_paste_splits_ipv4_domain_and_bracketed_ipv6() {
        assert_eq!(
            parse_host_port_paste("10.0.0.5:3306"),
            Some(("10.0.0.5".into(), "3306".into()))
        );
        assert_eq!(
            parse_host_port_paste("[2001:db8::1]:8443"),
            Some(("2001:db8::1".into(), "8443".into()))
        );
        assert_eq!(parse_host_port_paste("::1"), None);
        assert_eq!(parse_host_port_paste("example.com"), None);
    }

    #[test]
    fn save_pause_resume_and_remove_keep_json_clean_and_pinned_at_top() {
        let mut config = sample_config();
        let mut ws = Workspace::default();
        let mut form = PortForwardForm::from_new(&config, &[]);
        form.name = "内网数据库".into();
        form.local_port = "13306".into();
        form.target_host = "10.0.0.5".into();
        form.target_port = "3306".into();
        form.outbound_tag = "proxy-jp".into();

        let tag = save_entry(&mut config, &mut ws, "work", &form).unwrap();
        assert_eq!(tag, "pf-1");
        assert_eq!(config["routing"]["rules"][0]["inboundTag"], json!(["pf-1"]));
        assert_eq!(config["inbounds"][2]["protocol"], "dokodemo-door");

        // Pause removes from Core JSON while keeping parameters in workspace.
        let entries = extract_entries(&config, &ws, "work");
        assert_eq!(entries.len(), 1);
        set_entry_enabled(&mut config, &mut ws, "work", &entries[0], false).unwrap();
        assert_eq!(config["inbounds"].as_array().unwrap().len(), 2);
        assert_eq!(config["routing"]["rules"].as_array().unwrap().len(), 1);
        assert_eq!(ws.paused_port_forwards["work"].len(), 1);

        // Resume restores dokodemo-door + top routing rule.
        let entries = extract_entries(&config, &ws, "work");
        assert!(!entries[0].enabled);
        set_entry_enabled(&mut config, &mut ws, "work", &entries[0], true).unwrap();
        assert_eq!(config["inbounds"].as_array().unwrap().len(), 3);
        assert_eq!(config["routing"]["rules"][0]["inboundTag"], json!(["pf-1"]));
        assert!(!ws.paused_port_forwards.contains_key("work"));

        // Port conflict with http-in (10809) is rejected.
        let mut bad = PortForwardForm::from_new(&config, &[]);
        bad.local_port = "10809".into();
        bad.target_host = "10.0.0.5".into();
        bad.target_port = "3306".into();
        assert!(check_form_error(&config, &bad).is_some());
    }

    #[test]
    fn ssh_l_command_and_host_paste_parsers_and_auto_naming_work() {
        let parsed =
            parse_ssh_l_command("ssh -L 15432:10.0.1.8:5432 ops@bastion.corp -p 2222").unwrap();
        assert_eq!(parsed.listen, "127.0.0.1");
        assert_eq!(parsed.local_port, 15432);
        assert_eq!(parsed.target_host, "10.0.1.8");
        assert_eq!(parsed.target_port, 5432);
        assert_eq!(parsed.user.as_deref(), Some("ops"));
        assert_eq!(parsed.jump_host.as_deref(), Some("bastion.corp"));
        assert_eq!(parsed.ssh_port, 2222);

        let lan_parsed =
            parse_ssh_l_command("ssh -p2200 -L 0.0.0.0:16379:[::1]:6379 bastion.internal").unwrap();
        assert_eq!(lan_parsed.listen, "0.0.0.0");
        assert_eq!(lan_parsed.local_port, 16379);
        assert_eq!(lan_parsed.target_host, "::1");
        assert_eq!(lan_parsed.target_port, 6379);
        assert_eq!(lan_parsed.jump_host.as_deref(), Some("bastion.internal"));
        assert_eq!(lan_parsed.ssh_port, 2200);
        assert!(parse_ssh_l_command("ssh ops@bastion.corp").is_err());

        assert_eq!(
            parse_ssh_host_paste("ops@bastion.corp:2222"),
            Some((Some("ops".into()), "bastion.corp".into(), Some(2222)))
        );
        assert_eq!(
            parse_ssh_host_paste("bastion.corp:2222"),
            Some((None, "bastion.corp".into(), Some(2222)))
        );
        assert_eq!(
            parse_ssh_host_paste("ops@bastion.corp"),
            Some((Some("ops".into()), "bastion.corp".into(), None))
        );
        assert_eq!(parse_ssh_host_paste("bastion.corp"), None);

        assert_eq!(
            auto_ssh_forward_name("10.0.1.8", 3306),
            "MySQL (10.0.1.8:3306)"
        );
        assert_eq!(auto_ssh_forward_name("10.0.1.8", 8443), "10.0.1.8:8443");
        assert_eq!(
            auto_ssh_credential_name("ops", SshAuthKind::PrivateKey),
            "ops · 私钥"
        );
        assert_eq!(
            auto_ssh_credential_name("ops", SshAuthKind::Password),
            "ops · 密码"
        );

        let config = sample_config();
        let ssh_fwds = vec![SshForward {
            id: "sf-1".into(),
            name: "PG".into(),
            listen: "127.0.0.1".into(),
            port: 15432,
            target_host: "10.0.1.8".into(),
            target_port: 5432,
            ssh_host_id: "h-1".into(),
        }];
        let hosts = vec![SshHost {
            id: "h-1".into(),
            name: "堡垒".into(),
            host: "bastion.corp".into(),
            port: 2222,
            credential_id: "c-1".into(),
            host_key_fingerprint: "SHA256:abc".into(),
        }];
        assert_eq!(
            match_ssh_host_id(&hosts, "BASTION.CORP", 2222).as_deref(),
            Some("h-1")
        );
        assert_eq!(
            next_free_port_all(15432, Some(&config), &[], &ssh_fwds, None, None),
            15433
        );

        // Blank SSH name passes optional-config validation (even when JSON is None) and auto-names.
        let mut form = PortForwardForm::from_new_ssh(None, &[], &ssh_fwds, Some("h-1"));
        form.target_host = "10.0.1.9".into();
        form.target_port = "6379".into();
        form.local_port = "16379".into();
        assert!(check_form_error_with_optional_config(None, &form, &hosts, &ssh_fwds).is_none());
        let saved_fwd = ssh_forward_from_form(&form);
        assert_eq!(saved_fwd.name, "Redis (10.0.1.9:6379)");

        // Same port conflicting with existing SSH forward is caught.
        form.local_port = "15432".into();
        assert!(find_local_port_conflict(None, &form, &ssh_fwds).is_some());
    }
}
