use super::*;
#[cfg(windows)]
use proxy_config::bundle::encode_bundle_json;
use proxy_config::{
    bundle::{
        BundleEnvelope, BundlePayload, BundleProfile, BundleProxyPolicy, BundleSshHost,
        BundleUiMetadata, ParsedBundleInput, SHARE_CODE_PREFIX, build_ssh_topology, decrypt_bundle,
        encode_bundle_code, parse_bundle_input, summarize_payload,
    },
    resources::absolute_resource_paths,
    share_link::{EncodedShareBatch, encode_share_links, format_share_links_text},
};
use qrcodegen::{QrCode, QrCodeEcc};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::app) enum ShareTab {
    #[default]
    NodeLinks,
    Bundle,
    RawJson,
}

pub(in crate::app) type QrMatrixResult = std::result::Result<(usize, Vec<bool>), String>;
pub(in crate::app) type QrCacheEntry = (String, QrMatrixResult);

#[derive(Clone, Debug)]
pub(in crate::app) struct ShareDialogState {
    pub tab: ShareTab,
    pub selected_node_idx: usize,
    pub batch_base64: bool,
    pub qr_cache: Option<QrCacheEntry>,
    pub include_ui_metadata: bool,
    pub include_proxy_policy: bool,
    pub include_ssh_topology: bool,
    pub use_passphrase: bool,
    pub passphrase: String,
    pub passphrase_confirm: String,
    pub show_passphrase: bool,
    pub generated_code: Option<String>,
    pub feedback: Option<(bool, String)>,
}

impl Default for ShareDialogState {
    fn default() -> Self {
        Self {
            tab: ShareTab::NodeLinks,
            selected_node_idx: 0,
            batch_base64: false,
            qr_cache: None,
            include_ui_metadata: true,
            include_proxy_policy: true,
            include_ssh_topology: false,
            use_passphrase: false,
            passphrase: String::new(),
            passphrase_confirm: String::new(),
            show_passphrase: false,
            generated_code: None,
            feedback: None,
        }
    }
}

impl ShareDialogState {
    pub fn clear_secrets(&mut self) {
        self.passphrase.clear();
        self.passphrase_confirm.clear();
        self.show_passphrase = false;
        self.generated_code = None;
        self.feedback = None;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) enum BundleSshHostResolution {
    ReuseHost(String),
    BindCredential(String),
    InlineCredential {
        name: String,
        username: String,
        auth_kind: SshAuthKind,
        password: String,
        private_key_path: String,
        passphrase: String,
        show_secret: bool,
    },
    Skip,
}

impl BundleSshHostResolution {
    fn mode_label(&self) -> &'static str {
        match self {
            Self::ReuseHost(_) => "复用已有跳板机",
            Self::BindCredential(_) => "绑定已有凭证",
            Self::InlineCredential { .. } => "新建并保存凭证",
            Self::Skip => "跳过此跳板机",
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::app) struct BundleImportState {
    pub raw_input: String,
    pub encrypted_envelope: Option<BundleEnvelope>,
    pub decrypt_passphrase: String,
    pub show_passphrase: bool,
    pub payload: Option<BundlePayload>,
    pub apply_ui_metadata: bool,
    pub apply_proxy_policy: bool,
    pub apply_ssh_topology: bool,
    pub ssh_resolutions: BTreeMap<String, BundleSshHostResolution>,
    pub ssh_synced: bool,
    pub error: Option<String>,
}

impl Default for BundleImportState {
    fn default() -> Self {
        Self {
            raw_input: String::new(),
            encrypted_envelope: None,
            decrypt_passphrase: String::new(),
            show_passphrase: false,
            payload: None,
            apply_ui_metadata: true,
            apply_proxy_policy: true,
            apply_ssh_topology: true,
            ssh_resolutions: BTreeMap::new(),
            ssh_synced: false,
            error: None,
        }
    }
}

impl BundleImportState {
    pub fn clear_secrets(&mut self) {
        self.raw_input.clear();
        self.encrypted_envelope = None;
        self.decrypt_passphrase.clear();
        self.show_passphrase = false;
        self.payload = None;
        self.ssh_resolutions.clear();
        self.ssh_synced = false;
        self.error = None;
    }

    pub fn is_active(&self) -> bool {
        self.encrypted_envelope.is_some() || self.payload.is_some()
    }
}

fn utc_timestamp_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;

    // Howard Hinnant's civil-from-days algorithm
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn qr_matrix_for<'a>(state: &'a mut ShareDialogState, uri: &str) -> &'a QrMatrixResult {
    let needs_update = state
        .qr_cache
        .as_ref()
        .is_none_or(|(cached_uri, _)| cached_uri != uri);
    if needs_update {
        let encoded = QrCode::encode_text(uri, QrCodeEcc::Medium)
            .or_else(|_| QrCode::encode_text(uri, QrCodeEcc::Low))
            .map(|qr| {
                let size = qr.size() as usize;
                let mut modules = Vec::with_capacity(size * size);
                for y in 0..qr.size() {
                    for x in 0..qr.size() {
                        modules.push(qr.get_module(x, y));
                    }
                }
                (size, modules)
            })
            .map_err(|_| {
                "当前节点链接过长，超出二维码容量上限；请直接复制上方文本链接或使用完整配置包分享。"
                    .to_owned()
            });
        state.qr_cache = Some((uri.to_owned(), encoded));
    }
    &state.qr_cache.as_ref().unwrap().1
}

fn draw_qr_code(ui: &mut egui::Ui, size: usize, modules: &[bool], max_side: f32) {
    let quiet = 4usize;
    let total = size + quiet * 2;
    let cell = (max_side / total as f32).floor().max(2.0);
    let rendered = cell * total as f32;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(rendered, rendered), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        6.0,
        Color32::WHITE,
        egui::Stroke::new(1.0, Color32::from_gray(208)),
        egui::StrokeKind::Inside,
    );
    for y in 0..size {
        for x in 0..size {
            if modules[y * size + x] {
                let module_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        rect.min.x + (x + quiet) as f32 * cell,
                        rect.min.y + (y + quiet) as f32 * cell,
                    ),
                    egui::vec2(cell, cell),
                );
                painter.rect_filled(module_rect, 0.0, Color32::BLACK);
            }
        }
    }
}

fn default_ssh_resolution_for(
    host: &BundleSshHost,
    ssh_state: &SshState,
) -> BundleSshHostResolution {
    if let Some(existing) = ssh_state.hosts.iter().find(|h| {
        h.host.eq_ignore_ascii_case(&host.host)
            && h.port == host.port
            && (host.host_key_fingerprint.is_empty()
                || h.host_key_fingerprint.is_empty()
                || h.host_key_fingerprint == host.host_key_fingerprint)
    }) {
        return BundleSshHostResolution::ReuseHost(existing.id.clone());
    }
    let hint = host.credential_hint.as_ref();
    if !ssh_state.credentials.is_empty() {
        let preferred = hint
            .and_then(|h| {
                ssh_state
                    .credentials
                    .iter()
                    .find(|c| c.username == h.username && c.auth_kind == h.auth_kind)
                    .or_else(|| {
                        ssh_state
                            .credentials
                            .iter()
                            .find(|c| c.username == h.username)
                    })
            })
            .unwrap_or(&ssh_state.credentials[0]);
        return BundleSshHostResolution::BindCredential(preferred.id.clone());
    }
    BundleSshHostResolution::InlineCredential {
        name: String::new(),
        username: hint.map(|h| h.username.clone()).unwrap_or_default(),
        auth_kind: hint.map(|h| h.auth_kind).unwrap_or(SshAuthKind::Password),
        password: String::new(),
        private_key_path: String::new(),
        passphrase: String::new(),
        show_secret: false,
    }
}

fn build_ssh_resolutions(
    payload: &BundlePayload,
    ssh_state: &SshState,
) -> BTreeMap<String, BundleSshHostResolution> {
    let mut map = BTreeMap::new();
    if let Some(ssh) = &payload.ssh_topology {
        for host in &ssh.hosts {
            map.insert(
                host.ref_id.clone(),
                default_ssh_resolution_for(host, ssh_state),
            );
        }
    }
    map
}

fn pac_mode_short(mode: PacMode) -> &'static str {
    match mode {
        PacMode::AutoRules => "按规则自动分流",
        PacMode::DirectList => "黑名单直连",
        PacMode::ProxyList => "白名单代理",
        PacMode::All => "全部走代理",
        PacMode::Custom => "自定义脚本",
    }
}

impl App {
    pub(super) fn open_share_dialog(&mut self, tab: ShareTab, node_outbound_index: Option<usize>) {
        self.ui.share.clear_secrets();
        self.ui.share.tab = tab;
        self.ui.share.selected_node_idx = 0;
        if let Some(target_idx) = node_outbound_index
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(&self.editor)
        {
            let names = self
                .ui
                .workspace
                .node_names
                .get(&self.profile_id)
                .cloned()
                .unwrap_or_default();
            let batch = encode_share_links(&value, &names);
            if let Some(pos) = batch.nodes.iter().position(|n| n.index == target_idx) {
                self.ui.share.selected_node_idx = pos;
            }
        }
        if !self.ui.ssh_loaded {
            self.send(Command::GetSshState);
        }
        self.ui.dialog = Some(Dialog::Share);
    }

    fn current_bundle_payload(&self, parsed_config: &serde_json::Value) -> BundlePayload {
        let suggested_name = self.ui.workspace.name(&self.profile_id).to_owned();
        let ui_metadata = self.ui.share.include_ui_metadata.then(|| BundleUiMetadata {
            node_names: self
                .ui
                .workspace
                .node_names
                .get(&self.profile_id)
                .cloned()
                .unwrap_or_default(),
            port_forward_names: self
                .ui
                .workspace
                .port_forward_names
                .get(&self.profile_id)
                .cloned()
                .unwrap_or_default(),
            paused_port_forwards: self
                .ui
                .workspace
                .paused_port_forwards
                .get(&self.profile_id)
                .cloned()
                .unwrap_or_default(),
        });
        let proxy_policy = self.ui.share.include_proxy_policy.then(|| {
            let proxy_mode = self.proxy_mode_for(&self.profile_id);
            let pac = self.ui.workspace.pac.get(&self.profile_id).cloned();
            BundleProxyPolicy { proxy_mode, pac }
        });
        let ssh_topology = if self.ui.share.include_ssh_topology
            && (!self.ui.ssh_state.hosts.is_empty() || !self.ui.ssh_state.forwards.is_empty())
        {
            Some(build_ssh_topology(&self.ui.ssh_state))
        } else {
            None
        };
        BundlePayload {
            profile: BundleProfile {
                suggested_name,
                config_json: parsed_config.clone(),
            },
            ui_metadata,
            proxy_policy,
            ssh_topology,
        }
    }

    pub(super) fn share_dialog_contents(&mut self, ui: &mut egui::Ui) -> bool {
        let profile_name = self.ui.workspace.name(&self.profile_id).to_owned();
        dialogs::dialog_title(
            ui,
            &format!("分享 / 导出「{profile_name}」"),
            "支持生成通用节点链接与二维码（兼容手机/其他客户端）、Proxy Pilot 加密配置包，或导出原生 Xray JSON。",
        );

        ui.horizontal_wrapped(|ui| {
            for (tab, label) in [
                (ShareTab::NodeLinks, "节点链接 / 二维码"),
                (ShareTab::Bundle, "完整配置包"),
                (ShareTab::RawJson, "原生 JSON"),
            ] {
                if crate::widgets::pill_tab(ui, label, self.ui.share.tab == tab).clicked() {
                    self.ui.share.tab = tab;
                    self.ui.share.feedback = None;
                }
            }
        });
        ui.add_space(6.0);

        let body_height = (ui.ctx().content_rect().height() - 230.0).max(180.0);
        egui::ScrollArea::vertical()
            .id_salt("share-dialog-body")
            .max_height(body_height)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                match self.ui.share.tab {
                    ShareTab::NodeLinks => self.share_node_links_tab(ui),
                    ShareTab::Bundle => self.share_bundle_tab(ui),
                    ShareTab::RawJson => self.share_raw_json_tab(ui),
                }
                if let Some((is_err, msg)) = &self.ui.share.feedback {
                    ui.add_space(6.0);
                    let tone = if *is_err {
                        crate::widgets::BadgeTone::Danger
                    } else {
                        crate::widgets::BadgeTone::Success
                    };
                    crate::widgets::banner_box(ui, tone, msg);
                }
            });

        dialogs::dialog_actions(ui);
        let mut keep_open = true;
        ui.horizontal_wrapped(|ui| {
            if ui.button("关闭").clicked() {
                keep_open = false;
            }
            if self.has_draft_edits() {
                ui.label(
                    RichText::new("提示：当前导出基于编辑器草稿内容（包含尚未落盘的修改）。")
                        .small()
                        .weak(),
                );
            }
        });
        keep_open
    }

    fn share_node_links_tab(&mut self, ui: &mut egui::Ui) {
        let parsed = match proxy_config::parse_document(self.editor.as_bytes()) {
            Ok(v) => v,
            Err(err) => {
                crate::widgets::banner_box(
                    ui,
                    crate::widgets::BadgeTone::Danger,
                    &format!("当前配置 JSON 语法有误，无法提取节点链接：{}", err.message),
                );
                return;
            }
        };
        let names = self
            .ui
            .workspace
            .node_names
            .get(&self.profile_id)
            .cloned()
            .unwrap_or_default();
        let batch: EncodedShareBatch = encode_share_links(&parsed, &names);
        crate::widgets::banner_box(
            ui,
            crate::widgets::BadgeTone::Warn,
            "分享链接包含节点服务器地址、端口及 UUID/密码等认证凭据，请仅发送给可信接收方。",
        );
        ui.add_space(6.0);

        if batch.nodes.is_empty() {
            ui.label("当前配置没有可编码为标准分享链接的代理节点（系统直连/阻断出站已跳过）。");
        } else {
            if self.ui.share.selected_node_idx >= batch.nodes.len() {
                self.ui.share.selected_node_idx = 0;
            }
            let selected = &batch.nodes[self.ui.share.selected_node_idx];
            ui.horizontal_wrapped(|ui| {
                ui.label("选择节点");
                egui::ComboBox::from_id_salt("share-node-picker")
                    .selected_text(format!("{} ({})", selected.name, selected.protocol))
                    .show_ui(ui, |ui| {
                        for (i, node) in batch.nodes.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.ui.share.selected_node_idx,
                                i,
                                format!("{} ({})", node.name, node.protocol),
                            );
                        }
                    });
                if primary(ui, "复制此节点链接", true) {
                    ui.ctx().copy_text(selected.uri.clone());
                    self.ui.share.feedback = Some((
                        false,
                        format!("已复制节点「{}」的分享链接。", selected.name),
                    ));
                }
            });

            if !selected.warnings.is_empty() {
                ui.add_space(4.0);
                for warn in &selected.warnings {
                    ui.colored_label(ui.visuals().warn_fg_color, format!("• {warn}"));
                }
            }

            ui.add_space(6.0);
            let qr_result = qr_matrix_for(&mut self.ui.share, &selected.uri).clone();
            let wide = ui.available_width() >= 520.0;
            let show_qr = |ui: &mut egui::Ui| match &qr_result {
                Ok((size, modules)) => {
                    draw_qr_code(ui, *size, modules, 210.0);
                }
                Err(msg) => {
                    crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Warn, msg);
                }
            };
            let show_uri = |ui: &mut egui::Ui, uri: &str| {
                ui.label(
                    RichText::new("标准分享链接（可直接粘贴到手机/桌面客户端）")
                        .small()
                        .weak(),
                );
                let mut uri_preview = uri.to_owned();
                ui.add(
                    egui::TextEdit::multiline(&mut uri_preview)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            };

            if wide {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        show_qr(ui);
                    });
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.set_min_width(ui.available_width());
                        show_uri(ui, &selected.uri);
                    });
                });
            } else {
                show_qr(ui);
                ui.add_space(6.0);
                show_uri(ui, &selected.uri);
            }

            if batch.nodes.len() > 1 {
                ui.add_space(8.0);
                ui.separator();
                ui.label(
                    RichText::new(format!("批量分享全部节点（共 {} 个）", batch.nodes.len()))
                        .strong(),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.ui.share.batch_base64, "编码为 Base64 订阅文本");
                    if ui
                        .button(format!("复制全部节点 ({} 个)", batch.nodes.len()))
                        .clicked()
                    {
                        let text =
                            format_share_links_text(&batch.nodes, self.ui.share.batch_base64);
                        ui.ctx().copy_text(text);
                        self.ui.share.feedback = Some((
                            false,
                            format!(
                                "已复制全部 {} 个节点链接{}。",
                                batch.nodes.len(),
                                if self.ui.share.batch_base64 {
                                    "（Base64 编码）"
                                } else {
                                    ""
                                }
                            ),
                        ));
                    }
                });
            }
        }

        if !batch.errors.is_empty() || !batch.skipped.is_empty() {
            ui.add_space(6.0);
            for (tag, err) in &batch.errors {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("未导出节点 {tag}：{err}"),
                );
            }
            if !batch.skipped.is_empty() {
                let skipped_names: Vec<_> =
                    batch.skipped.iter().map(|(tag, _)| tag.as_str()).collect();
                ui.label(
                    RichText::new(format!("已跳过系统出站：{}", skipped_names.join("、")))
                        .small()
                        .weak(),
                );
            }
        }
    }

    fn share_bundle_tab(&mut self, ui: &mut egui::Ui) {
        let parsed = match proxy_config::parse_document(self.editor.as_bytes()) {
            Ok(v) => v,
            Err(err) => {
                crate::widgets::banner_box(
                    ui,
                    crate::widgets::BadgeTone::Danger,
                    &format!("当前配置 JSON 语法有误，无法打包配置包：{}", err.message),
                );
                return;
            }
        };

        ui.label(RichText::new("配置包包含范围").strong());
        ui.checkbox(
            &mut self.ui.share.include_ui_metadata,
            "包含节点与端口转发显示名称及停用暂存项",
        );
        ui.checkbox(
            &mut self.ui.share.include_proxy_policy,
            "包含记忆的代理模式与 PAC 策略（规则、域名清单、端口）",
        );

        let ssh_host_count = self.ui.ssh_state.hosts.len();
        let ssh_fwd_count = self.ui.ssh_state.forwards.len();
        let has_ssh = ssh_host_count > 0 || ssh_fwd_count > 0;
        ui.add_enabled_ui(has_ssh, |ui| {
            ui.checkbox(
                &mut self.ui.share.include_ssh_topology,
                format!(
                    "包含 SSH 跳板机与隧道拓扑（{ssh_host_count} 台跳板机 / {ssh_fwd_count} 条隧道 · 不含密码与私钥）"
                ),
            )
            .on_disabled_hover_text("当前尚未配置 SSH 跳板机或隧道");
        });
        if !has_ssh {
            self.ui.share.include_ssh_topology = false;
        }

        ui.add_space(6.0);
        ui.separator();
        ui.label(RichText::new("安全保护").strong());
        ui.checkbox(
            &mut self.ui.share.use_passphrase,
            "使用口令加密（Argon2id + AES-256-GCM，推荐聊天工具传输时开启）",
        );
        if self.ui.share.use_passphrase {
            ui.horizontal_wrapped(|ui| {
                ui.label("加密口令");
                ui.add(
                    singleline(&mut self.ui.share.passphrase, 200.0)
                        .password(!self.ui.share.show_passphrase)
                        .hint_text("输入口令"),
                );
                ui.label("确认口令");
                ui.add(
                    singleline(&mut self.ui.share.passphrase_confirm, 200.0)
                        .password(!self.ui.share.show_passphrase)
                        .hint_text("再次输入"),
                );
                ui.checkbox(&mut self.ui.share.show_passphrase, "显示口令");
            });
            if self.ui.share.passphrase.is_empty() {
                ui.colored_label(ui.visuals().warn_fg_color, "请输入非空加密口令。");
            } else if self.ui.share.passphrase != self.ui.share.passphrase_confirm {
                ui.colored_label(ui.visuals().error_fg_color, "两次输入的口令不一致。");
            }
        } else {
            crate::widgets::banner_box(
                ui,
                crate::widgets::BadgeTone::Warn,
                "未启用口令加密：导出的配置包将明文包含节点地址、UUID/密码等凭据，建议仅在可信环境传输或勾选上方口令加密。",
            );
        }

        if self.ui.share.include_ssh_topology {
            ui.add_space(4.0);
            crate::widgets::banner_box(
                ui,
                crate::widgets::BadgeTone::Accent,
                "安全边界：SSH 密码、私钥路径与口令不会写入配置包；接收方导入时可选择复用已有跳板机或补填凭证。",
            );
        }
        let abs_paths = absolute_resource_paths(&parsed);
        if !abs_paths.is_empty() {
            ui.add_space(4.0);
            crate::widgets::banner_box(
                ui,
                crate::widgets::BadgeTone::Warn,
                &format!(
                    "配置引用了本机绝对路径资源（{}），跨机器导入后可能需要在接收方重新指定路径。",
                    abs_paths.join("、")
                ),
            );
        }

        let passphrase_valid = !self.ui.share.use_passphrase
            || (!self.ui.share.passphrase.is_empty()
                && self.ui.share.passphrase == self.ui.share.passphrase_confirm);

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if primary(ui, "复制分享码", passphrase_valid) {
                let payload = self.current_bundle_payload(&parsed);
                let secret = self
                    .ui
                    .share
                    .use_passphrase
                    .then_some(self.ui.share.passphrase.as_str());
                match encode_bundle_code(
                    &payload,
                    secret,
                    env!("CARGO_PKG_VERSION"),
                    &utc_timestamp_now(),
                ) {
                    Ok(code) => {
                        let len = code.len();
                        ui.ctx().copy_text(code.clone());
                        self.ui.share.generated_code = Some(code);
                        let hint = if len > 16 * 1024 {
                            "（分享码较长，若聊天软件截断请改用配置文件导出）"
                        } else {
                            ""
                        };
                        self.ui.share.feedback = Some((
                            false,
                            format!(
                                "已复制{}配置包分享码（{len} 字符）{hint}。",
                                if self.ui.share.use_passphrase {
                                    "加密"
                                } else {
                                    ""
                                }
                            ),
                        ));
                    }
                    Err(err) => {
                        self.ui.share.feedback = Some((true, err.message));
                    }
                }
            }
            #[cfg(windows)]
            if ui
                .add_enabled(passphrase_valid, egui::Button::new("导出配置包文件…"))
                .clicked()
            {
                let suggested_file = format!(
                    "{}.ppbundle.json",
                    sanitize_filename(self.ui.workspace.name(&self.profile_id))
                );
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Proxy Pilot 配置包", &["ppbundle.json", "json"])
                    .set_file_name(&suggested_file)
                    .save_file()
                {
                    let payload = self.current_bundle_payload(&parsed);
                    let secret = self
                        .ui
                        .share
                        .use_passphrase
                        .then_some(self.ui.share.passphrase.as_str());
                    match encode_bundle_json(
                        &payload,
                        secret,
                        env!("CARGO_PKG_VERSION"),
                        &utc_timestamp_now(),
                    ) {
                        Ok(text) => match std::fs::write(&path, text) {
                            Ok(()) => {
                                self.ui.share.feedback = Some((false, "配置包文件已导出。".into()));
                            }
                            Err(err) => {
                                self.ui.share.feedback =
                                    Some((true, format!("导出配置包失败：{err}")));
                            }
                        },
                        Err(err) => {
                            self.ui.share.feedback = Some((true, err.message));
                        }
                    }
                }
            }
            #[cfg(not(windows))]
            let _ = ui
                .add_enabled(false, egui::Button::new("导出配置包文件…"))
                .on_disabled_hover_text("当前平台不提供图形文件保存框，请点击「复制分享码」导入");
        });

        if let Some(code) = &self.ui.share.generated_code {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("分享码预览（{SHARE_CODE_PREFIX}...）"))
                    .small()
                    .weak(),
            );
            let mut preview = code.clone();
            ui.add(
                egui::TextEdit::multiline(&mut preview)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .interactive(false),
            );
        }
    }

    fn share_raw_json_tab(&mut self, ui: &mut egui::Ui) {
        crate::widgets::banner_box(
            ui,
            crate::widgets::BadgeTone::Warn,
            "原生 Xray 配置可能包含节点地址、UUID、密码、私钥路径等敏感信息；导出文件或复制文本不受本程序数据目录保护，请妥善保管。",
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if primary(ui, "复制原生 JSON", true) {
                ui.ctx().copy_text(self.editor.clone());
                self.ui.share.feedback = Some((false, "已复制当前配置 JSON 到剪贴板。".into()));
            }
            #[cfg(windows)]
            if ui.button("导出 JSON 文件…").clicked() {
                let suggested_file = format!(
                    "{}.json",
                    sanitize_filename(self.ui.workspace.name(&self.profile_id))
                );
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("原生 Xray JSON", &["json"])
                    .set_file_name(&suggested_file)
                    .save_file()
                {
                    if let Err(error) = std::fs::write(path, &self.editor) {
                        self.ui.share.feedback = Some((true, format!("导出 JSON 失败：{error}")));
                    } else {
                        self.ui.share.feedback =
                            Some((false, "配置已导出；文件可能包含认证信息。".into()));
                    }
                }
            }
            #[cfg(not(windows))]
            let _ = ui
                .add_enabled(false, egui::Button::new("导出 JSON 文件…"))
                .on_disabled_hover_text("当前平台不提供图形文件保存框，请点击「复制原生 JSON」");
        });
        ui.add_space(6.0);
        let mut raw_preview = self.editor.clone();
        ui.add(
            egui::TextEdit::multiline(&mut raw_preview)
                .font(egui::TextStyle::Monospace)
                .desired_rows(8)
                .desired_width(f32::INFINITY)
                .interactive(false),
        );
    }

    pub(super) fn try_parse_bundle_into_state(&mut self, input: &str) -> bool {
        self.ui.bundle_import.error = None;
        self.ui.bundle_import.encrypted_envelope = None;
        self.ui.bundle_import.payload = None;
        self.ui.bundle_import.decrypt_passphrase.clear();
        self.ui.bundle_import.ssh_resolutions.clear();
        self.ui.bundle_import.ssh_synced = false;
        match parse_bundle_input(input) {
            Ok(ParsedBundleInput::Plain(_, payload)) => {
                self.accept_decoded_bundle_payload(*payload);
                true
            }
            Ok(ParsedBundleInput::Encrypted(env)) => {
                self.ui.bundle_import.encrypted_envelope = Some(env);
                if !self.ui.ssh_loaded {
                    self.send(Command::GetSshState);
                }
                true
            }
            Err(err) => {
                self.ui.bundle_import.error = Some(err.message);
                false
            }
        }
    }

    fn accept_decoded_bundle_payload(&mut self, payload: BundlePayload) {
        if self.ui.new_name.trim().is_empty() && !payload.profile.suggested_name.trim().is_empty() {
            self.ui.new_name = payload
                .profile
                .suggested_name
                .trim()
                .chars()
                .take(80)
                .collect();
        }
        self.ui.bundle_import.apply_ui_metadata = payload.ui_metadata.is_some();
        self.ui.bundle_import.apply_proxy_policy = payload.proxy_policy.is_some();
        self.ui.bundle_import.apply_ssh_topology = payload
            .ssh_topology
            .as_ref()
            .is_some_and(|s| !s.hosts.is_empty());
        self.ui.bundle_import.ssh_resolutions = build_ssh_resolutions(&payload, &self.ui.ssh_state);
        self.ui.bundle_import.ssh_synced = self.ui.ssh_loaded;
        if payload.ssh_topology.is_some() && !self.ui.ssh_loaded {
            self.send(Command::GetSshState);
        }
        self.ui.bundle_import.payload = Some(payload);
        self.ui.bundle_import.error = None;
    }

    pub(super) fn bundle_import_body(&mut self, ui: &mut egui::Ui) -> (bool, bool) {
        if self.ui.ssh_loaded && !self.ui.bundle_import.ssh_synced {
            if let Some(payload) = &self.ui.bundle_import.payload {
                self.ui.bundle_import.ssh_resolutions =
                    build_ssh_resolutions(payload, &self.ui.ssh_state);
            }
            self.ui.bundle_import.ssh_synced = true;
        }

        if let Some(env) = self.ui.bundle_import.encrypted_envelope.clone()
            && self.ui.bundle_import.payload.is_none()
        {
            ui.add_space(6.0);
            crate::widgets::subcard(ui, Some(crate::widgets::BadgeTone::Accent), false, |ui| {
                ui.label(RichText::new("已识别加密配置包").strong());
                ui.label(
                    RichText::new(format!(
                        "版本 v{} · 导出于 {} · 来源客户端 {}",
                        env.schema_version, env.exported_at_utc, env.app_version
                    ))
                    .small()
                    .weak(),
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label("解密口令");
                    let resp = ui.add(
                        singleline(&mut self.ui.bundle_import.decrypt_passphrase, 220.0)
                            .password(!self.ui.bundle_import.show_passphrase)
                            .hint_text("输入导出时设置的口令"),
                    );
                    ui.checkbox(&mut self.ui.bundle_import.show_passphrase, "显示口令");
                    let submit = primary(
                        ui,
                        "解密并预览",
                        !self.ui.bundle_import.decrypt_passphrase.is_empty(),
                    ) || (resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !self.ui.bundle_import.decrypt_passphrase.is_empty());
                    if submit {
                        match decrypt_bundle(&env, &self.ui.bundle_import.decrypt_passphrase) {
                            Ok(payload) => {
                                self.ui.bundle_import.decrypt_passphrase.clear();
                                self.ui.bundle_import.show_passphrase = false;
                                self.accept_decoded_bundle_payload(payload);
                            }
                            Err(err) => {
                                self.ui.bundle_import.error = Some(err.message);
                            }
                        }
                    }
                    if ui.button("返回普通导入").clicked() {
                        self.ui.bundle_import.clear_secrets();
                    }
                });
                if let Some(err) = &self.ui.bundle_import.error {
                    ui.add_space(4.0);
                    ui.colored_label(ui.visuals().error_fg_color, err);
                }
            });
            return (false, false);
        }

        let Some(payload) = self.ui.bundle_import.payload.clone() else {
            return (false, false);
        };
        let summary = summarize_payload(&payload);
        let config_bytes = serde_json::to_vec(&payload.profile.config_json).unwrap_or_default();
        let needs_work = proxy_config::parse_document(&config_bytes)
            .is_ok_and(|value| !proxy_config::inspect_config(&value).is_empty());

        ui.add_space(6.0);
        crate::widgets::subcard(ui, Some(crate::widgets::BadgeTone::Success), false, |ui| {
            crate::widgets::responsive_header(
                ui,
                460.0,
                |ui| {
                    ui.label(RichText::new("配置包内容预览").strong());
                },
                |ui, _| {
                    if ui.button("改用其他文件 / 分享码").clicked() {
                        self.ui.bundle_import.clear_secrets();
                    }
                },
            );
            ui.label(format!(
                "• 代理节点：{} 个 · 分流规则：{} 条 · Xray 端口转发：{} 条",
                summary.proxy_node_count, summary.routing_rule_count, summary.dokodemo_count
            ));
            if let Some(ui_meta) = &payload.ui_metadata
                && summary.has_ui_metadata
            {
                let mut desc = format!(
                    "节点/转发显示名称（{} + {} 项）",
                    ui_meta.node_names.len(),
                    ui_meta.port_forward_names.len()
                );
                if summary.paused_pf_count > 0 {
                    desc.push_str(&format!("、停用转发暂存 {} 条", summary.paused_pf_count));
                }
                ui.checkbox(
                    &mut self.ui.bundle_import.apply_ui_metadata,
                    format!("恢复{desc}"),
                );
            }
            if let Some(policy) = &payload.proxy_policy {
                let mut desc = format!("记忆代理模式：{}", mode_name(policy.proxy_mode));
                if let Some(pac) = &policy.pac {
                    desc.push_str(&format!(
                        "（PAC {} · {} 个域名规则 · 端口 {}）",
                        pac_mode_short(pac.mode),
                        pac.domains.len(),
                        pac.port
                    ));
                }
                ui.checkbox(
                    &mut self.ui.bundle_import.apply_proxy_policy,
                    format!("恢复{desc}"),
                );
            }
            if let Some(ssh) = &payload.ssh_topology
                && (!ssh.hosts.is_empty() || !ssh.forwards.is_empty())
            {
                ui.checkbox(
                    &mut self.ui.bundle_import.apply_ssh_topology,
                    format!(
                        "导入 SSH 跳板机与隧道拓扑（{} 台跳板机 / {} 条本地转发）",
                        ssh.hosts.len(),
                        ssh.forwards.len()
                    ),
                );
            }
        });

        if !summary.absolute_resource_paths.is_empty() {
            ui.add_space(4.0);
            crate::widgets::banner_box(
                ui,
                crate::widgets::BadgeTone::Warn,
                &format!(
                    "配置包引用了本机绝对路径资源：{}；若本机路径不同，请在导入后到「分流规则 / 本地入口」或设置资源目录调整。",
                    summary.absolute_resource_paths.join("、")
                ),
            );
        }

        let mut ssh_valid = true;
        if self.ui.bundle_import.apply_ssh_topology
            && let Some(ssh) = &payload.ssh_topology
            && !ssh.hosts.is_empty()
        {
            ui.add_space(6.0);
            ui.label(RichText::new("SSH 跳板机凭证绑定（配置包不含任何 SSH 密码或私钥）").strong());
            for host in &ssh.hosts {
                let fwd_count = ssh
                    .forwards
                    .iter()
                    .filter(|f| f.host_ref == host.ref_id)
                    .count();
                let res = self
                    .ui
                    .bundle_import
                    .ssh_resolutions
                    .entry(host.ref_id.clone())
                    .or_insert_with(|| default_ssh_resolution_for(host, &self.ui.ssh_state));

                ui.add_space(4.0);
                crate::widgets::subcard(ui, None, false, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&host.name).strong());
                        ui.monospace(format!("{}:{}", host.host, host.port));
                        crate::widgets::badge(
                            ui,
                            &format!("{fwd_count} 条关联隧道"),
                            crate::widgets::BadgeTone::Muted,
                        );
                    });
                    if !host.host_key_fingerprint.is_empty() {
                        ui.label(
                            RichText::new(format!("指纹：{}", host.host_key_fingerprint))
                                .small()
                                .monospace()
                                .weak(),
                        );
                    }
                    if let Some(hint) = &host.credential_hint {
                        let kind = match hint.auth_kind {
                            SshAuthKind::Password => "密码认证",
                            SshAuthKind::PrivateKey => "私钥认证",
                        };
                        ui.label(
                            RichText::new(format!("建议账号：{}（{kind}）", hint.username))
                                .small()
                                .weak(),
                        );
                    }

                    let mut mode_tag = match res {
                        BundleSshHostResolution::ReuseHost(_) => 0usize,
                        BundleSshHostResolution::BindCredential(_) => 1usize,
                        BundleSshHostResolution::InlineCredential { .. } => 2usize,
                        BundleSshHostResolution::Skip => 3usize,
                    };
                    let prev_tag = mode_tag;
                    ui.horizontal_wrapped(|ui| {
                        ui.label("处理方式");
                        egui::ComboBox::from_id_salt(("bundle-ssh-mode", &host.ref_id))
                            .selected_text(res.mode_label())
                            .show_ui(ui, |ui| {
                                if !self.ui.ssh_state.hosts.is_empty() {
                                    ui.selectable_value(&mut mode_tag, 0, "复用已有跳板机");
                                }
                                if !self.ui.ssh_state.credentials.is_empty() {
                                    ui.selectable_value(&mut mode_tag, 1, "绑定已有凭证");
                                }
                                ui.selectable_value(&mut mode_tag, 2, "新建并保存凭证");
                                ui.selectable_value(&mut mode_tag, 3, "跳过此跳板机");
                            });
                    });
                    if mode_tag != prev_tag {
                        let hint = host.credential_hint.as_ref();
                        *res = match mode_tag {
                            0 => BundleSshHostResolution::ReuseHost(
                                self.ui
                                    .ssh_state
                                    .hosts
                                    .first()
                                    .map(|h| h.id.clone())
                                    .unwrap_or_default(),
                            ),
                            1 => BundleSshHostResolution::BindCredential(
                                self.ui
                                    .ssh_state
                                    .credentials
                                    .first()
                                    .map(|c| c.id.clone())
                                    .unwrap_or_default(),
                            ),
                            2 => BundleSshHostResolution::InlineCredential {
                                name: String::new(),
                                username: hint.map(|h| h.username.clone()).unwrap_or_default(),
                                auth_kind: hint
                                    .map(|h| h.auth_kind)
                                    .unwrap_or(SshAuthKind::Password),
                                password: String::new(),
                                private_key_path: String::new(),
                                passphrase: String::new(),
                                show_secret: false,
                            },
                            _ => BundleSshHostResolution::Skip,
                        };
                    }

                    match res {
                        BundleSshHostResolution::ReuseHost(host_id) => {
                            let selected_label = self
                                .ui
                                .ssh_state
                                .hosts
                                .iter()
                                .find(|h| h.id == *host_id)
                                .map(|h| format!("{} ({}:{})", h.name, h.host, h.port))
                                .unwrap_or_else(|| "请选择已有跳板机".into());
                            egui::ComboBox::from_id_salt(("bundle-ssh-reuse", &host.ref_id))
                                .selected_text(selected_label)
                                .show_ui(ui, |ui| {
                                    for h in &self.ui.ssh_state.hosts {
                                        ui.selectable_value(
                                            host_id,
                                            h.id.clone(),
                                            format!("{} ({}:{})", h.name, h.host, h.port),
                                        );
                                    }
                                });
                            if !self.ui.ssh_state.hosts.iter().any(|h| h.id == *host_id) {
                                ssh_valid = false;
                            }
                        }
                        BundleSshHostResolution::BindCredential(cred_id) => {
                            let selected_label = self
                                .ui
                                .ssh_state
                                .credentials
                                .iter()
                                .find(|c| c.id == *cred_id)
                                .map(|c| format!("{} ({})", c.name, c.username))
                                .unwrap_or_else(|| "请选择已有凭证".into());
                            egui::ComboBox::from_id_salt(("bundle-ssh-bind", &host.ref_id))
                                .selected_text(selected_label)
                                .show_ui(ui, |ui| {
                                    for c in &self.ui.ssh_state.credentials {
                                        ui.selectable_value(
                                            cred_id,
                                            c.id.clone(),
                                            format!("{} ({})", c.name, c.username),
                                        );
                                    }
                                });
                            if !self
                                .ui
                                .ssh_state
                                .credentials
                                .iter()
                                .any(|c| c.id == *cred_id)
                            {
                                ssh_valid = false;
                            }
                        }
                        BundleSshHostResolution::InlineCredential {
                            name,
                            username,
                            auth_kind,
                            password,
                            private_key_path,
                            passphrase,
                            show_secret,
                        } => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("凭证名称");
                                ui.add(singleline(name, 140.0).hint_text("留空自动命名"));
                                ui.label("账号");
                                ui.add(singleline(username, 120.0).hint_text("如 ops"));
                                ui.selectable_value(auth_kind, SshAuthKind::Password, "密码");
                                ui.selectable_value(auth_kind, SshAuthKind::PrivateKey, "私钥");
                            });
                            match auth_kind {
                                SshAuthKind::Password => {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label("登录密码");
                                        ui.add(
                                            singleline(password, 180.0)
                                                .password(!*show_secret)
                                                .hint_text("必填"),
                                        );
                                        ui.checkbox(show_secret, "显示");
                                    });
                                    if username.trim().is_empty() || password.is_empty() {
                                        ssh_valid = false;
                                        ui.colored_label(
                                            ui.visuals().warn_fg_color,
                                            "请填写 SSH 账号和登录密码。",
                                        );
                                    }
                                }
                                SshAuthKind::PrivateKey => {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label("私钥路径");
                                        ui.add(
                                            singleline(private_key_path, 220.0)
                                                .hint_text("例如 C:\\Users\\...\\id_ed25519"),
                                        );
                                        #[cfg(windows)]
                                        if ui.button("选择私钥…").clicked()
                                            && let Some(path) = rfd::FileDialog::new().pick_file()
                                        {
                                            *private_key_path = to_portable_string(
                                                &self.root,
                                                &path.to_string_lossy(),
                                            );
                                        }
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label("私钥口令");
                                        ui.add(
                                            singleline(passphrase, 180.0)
                                                .password(!*show_secret)
                                                .hint_text("无口令可留空"),
                                        );
                                        ui.checkbox(show_secret, "显示");
                                    });
                                    if username.trim().is_empty()
                                        || private_key_path.trim().is_empty()
                                    {
                                        ssh_valid = false;
                                        ui.colored_label(
                                            ui.visuals().warn_fg_color,
                                            "请填写 SSH 账号和私钥文件路径。",
                                        );
                                    }
                                }
                            }
                        }
                        BundleSshHostResolution::Skip => {
                            if fwd_count > 0 {
                                ui.label(
                                    RichText::new(format!(
                                        "已跳过：关联的 {fwd_count} 条 SSH 隧道将不会导入。"
                                    ))
                                    .small()
                                    .weak(),
                                );
                            }
                        }
                    }
                });
            }

            // Warn about local port collisions with existing SSH forwards
            let conflicts: Vec<String> = ssh
                .forwards
                .iter()
                .filter(|f| {
                    !matches!(
                        self.ui.bundle_import.ssh_resolutions.get(&f.host_ref),
                        Some(BundleSshHostResolution::Skip)
                    ) && self
                        .ui
                        .ssh_state
                        .forwards
                        .iter()
                        .any(|existing| existing.listen == f.listen && existing.port == f.port)
                })
                .map(|f| format!("{} ({}:{})", f.name, f.listen, f.port))
                .collect();
            if !conflicts.is_empty() {
                ui.add_space(4.0);
                crate::widgets::banner_box(
                    ui,
                    crate::widgets::BadgeTone::Warn,
                    &format!(
                        "以下待导入 SSH 隧道本地端口与现有规则冲突，导入可能被后台拒绝：{}",
                        conflicts.join("、")
                    ),
                );
            }
        }

        (ssh_valid, needs_work)
    }

    pub(super) fn confirm_bundle_import(&mut self) {
        let Some(payload) = self.ui.bundle_import.payload.clone() else {
            return;
        };
        let profile_id = format!("profile-{}", id());
        let display_name: String = self.ui.new_name.trim().chars().take(80).collect();
        self.ui
            .workspace
            .names
            .insert(profile_id.clone(), display_name);
        if self.ui.bundle_import.apply_ui_metadata
            && let Some(meta) = payload.ui_metadata
        {
            if !meta.node_names.is_empty() {
                self.ui
                    .workspace
                    .node_names
                    .insert(profile_id.clone(), meta.node_names);
            }
            if !meta.port_forward_names.is_empty() {
                self.ui
                    .workspace
                    .port_forward_names
                    .insert(profile_id.clone(), meta.port_forward_names);
            }
            if !meta.paused_port_forwards.is_empty() {
                self.ui
                    .workspace
                    .paused_port_forwards
                    .insert(profile_id.clone(), meta.paused_port_forwards);
            }
        }
        if self.ui.bundle_import.apply_proxy_policy
            && let Some(policy) = payload.proxy_policy
        {
            self.ui
                .workspace
                .proxy_modes
                .insert(profile_id.clone(), policy.proxy_mode);
            if let Some(pac) = policy.pac {
                self.ui.workspace.pac.insert(profile_id.clone(), pac);
            }
        }
        self.save_workspace();

        let json = serde_json::to_string_pretty(&payload.profile.config_json)
            .unwrap_or_else(|_| "{}".into());
        self.enqueue_write(Command::CreateProfile { profile_id, json });

        let mut ssh_host_created = 0usize;
        let mut ssh_fwd_created = 0usize;
        if self.ui.bundle_import.apply_ssh_topology
            && let Some(ssh) = payload.ssh_topology
        {
            let mut host_id_map: HashMap<String, String> = HashMap::new();
            for host in ssh.hosts {
                let resolution = self
                    .ui
                    .bundle_import
                    .ssh_resolutions
                    .get(&host.ref_id)
                    .cloned()
                    .unwrap_or(BundleSshHostResolution::Skip);
                match resolution {
                    BundleSshHostResolution::ReuseHost(existing_id) => {
                        if !existing_id.is_empty() {
                            host_id_map.insert(host.ref_id, existing_id);
                        }
                    }
                    BundleSshHostResolution::BindCredential(cred_id) => {
                        if !cred_id.is_empty() {
                            let new_host_id = id();
                            self.enqueue_write(Command::SaveSshHost {
                                host: SshHost {
                                    id: new_host_id.clone(),
                                    name: host.name,
                                    host: host.host,
                                    port: host.port,
                                    credential_id: cred_id,
                                    host_key_fingerprint: host.host_key_fingerprint,
                                },
                                inline_credential: None,
                            });
                            host_id_map.insert(host.ref_id, new_host_id);
                            ssh_host_created += 1;
                        }
                    }
                    BundleSshHostResolution::InlineCredential {
                        name,
                        username,
                        auth_kind,
                        password,
                        private_key_path,
                        passphrase,
                        ..
                    } => {
                        let cred_name = if name.trim().is_empty() {
                            crate::port_forward::auto_ssh_credential_name(&username, auth_kind)
                        } else {
                            name.trim().to_owned()
                        };
                        let inline = SshCredentialInput {
                            id: String::new(),
                            name: cred_name,
                            username: username.trim().to_owned(),
                            auth_kind,
                            password: (auth_kind == SshAuthKind::Password && !password.is_empty())
                                .then_some(password),
                            private_key_path: if auth_kind == SshAuthKind::PrivateKey {
                                private_key_path.trim().into()
                            } else {
                                Default::default()
                            },
                            passphrase: (auth_kind == SshAuthKind::PrivateKey
                                && !passphrase.is_empty())
                            .then_some(passphrase),
                        };
                        let new_host_id = id();
                        self.enqueue_write(Command::SaveSshHost {
                            host: SshHost {
                                id: new_host_id.clone(),
                                name: host.name,
                                host: host.host,
                                port: host.port,
                                credential_id: String::new(),
                                host_key_fingerprint: host.host_key_fingerprint,
                            },
                            inline_credential: Some(inline),
                        });
                        host_id_map.insert(host.ref_id, new_host_id);
                        ssh_host_created += 1;
                    }
                    BundleSshHostResolution::Skip => {}
                }
            }
            for fwd in ssh.forwards {
                if let Some(ssh_host_id) = host_id_map.get(&fwd.host_ref) {
                    self.enqueue_write(Command::SaveSshForward {
                        forward: SshForward {
                            id: String::new(),
                            name: fwd.name,
                            listen: fwd.listen,
                            port: fwd.port,
                            target_host: fwd.target_host,
                            target_port: fwd.target_port,
                            ssh_host_id: ssh_host_id.clone(),
                        },
                        start_now: false,
                    });
                    ssh_fwd_created += 1;
                }
            }
        }

        self.ui.bundle_import.clear_secrets();
        if ssh_host_created > 0 || ssh_fwd_created > 0 {
            self.message = format!(
                "已导入配置包，并提交 {ssh_host_created} 台跳板机与 {ssh_fwd_created} 条 SSH 隧道；尚未应用。"
            );
        } else {
            self.message = "已导入配置包，尚未应用。".into();
        }
    }
}

#[cfg(windows)]
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "proxy-pilot-profile".into()
    } else {
        trimmed.chars().take(64).collect()
    }
}
