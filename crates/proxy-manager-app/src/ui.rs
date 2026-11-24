use super::*;
use crate::{
    path_display::{path as display_path, text as display_path_text},
    preferences::Appearance,
    widgets::{CONTROL_HEIGHT, singleline},
    workspace::{Draft, Workspace},
};
use egui::{Color32, RichText};

#[path = "ui/configuration.rs"]
mod configuration;
#[path = "ui/diagnostics.rs"]
mod diagnostics;
#[path = "ui/dialogs.rs"]
mod dialogs;
use configuration::{config_review, config_review_action, json_editor, json_viewer};
#[path = "ui/layout.rs"]
mod layout;
#[path = "ui/pac.rs"]
mod pac;
#[path = "ui/profiles.rs"]
mod profiles;
#[path = "ui/settings.rs"]
mod settings;
#[path = "ui/share.rs"]
mod share;
#[cfg(test)]
#[path = "ui/tests.rs"]
mod tests;

#[derive(Clone, Debug)]
pub(super) struct GeoDataCache {
    pub asset_root: String,
    pub stamp: u64,
    pub data: proxy_config::geodata::GeoData,
    pub categories: Vec<proxy_config::geodata::GeoSiteCategorySummary>,
    pub geosite_error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    Overview,
    Profiles,
    Pac,
    Logs,
    Diagnostics,
    Settings,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ProfileTab {
    #[default]
    Outbounds,
    Routing,
    PortForward,
    Advanced,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AdvancedTab {
    #[default]
    Inbounds,
    Json,
    History,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SettingsTab {
    #[default]
    Core,
    Startup,
    Probe,
    Appearance,
    Storage,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PacDomainFilter {
    #[default]
    All,
    DomainAndSub,
    Subdomain,
    Exact,
}
#[derive(Clone)]
pub(super) enum Action {
    Switch(String),
    Discard(String),
    Delete { profile: String, revision: String },
    Stop,
    Restart,
    Proxy(ProxyMode),
    AdoptDraft(String),
    ClearPacDomains(String),
    ResolveProxyConflict,
    ResolveStartupConflict,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExitKind {
    Gui,
    Shutdown,
}

/// Two-frame state machine that keeps `--background` launches hidden despite `eframe` showing frame 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(super) enum BackgroundHide {
    #[default]
    None,
    HideAfterFirstFrame,
    UncloakAfterHide,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PortForwardSubTab {
    #[default]
    Forwards,
    SshHosts,
    SshCredentials,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SshCredentialForm {
    pub editing_id: Option<String>,
    pub name: String,
    pub username: String,
    pub auth_kind: proxy_model::SshAuthKind,
    pub password: String,
    pub has_existing_password: bool,
    pub private_key_path: String,
    pub passphrase: String,
    pub has_existing_passphrase: bool,
    pub clear_passphrase: bool,
    pub show_secret: bool,
    pub error: Option<String>,
    pub return_to_host: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SshHostCredMode {
    #[default]
    Reuse,
    Inline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SshHostTestStatus {
    Testing,
    Ok(String),
    Error(String),
}

#[derive(Clone, Debug)]
pub(super) struct SshHostForm {
    pub editing_id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: String,
    pub cred_mode: SshHostCredMode,
    pub credential_id: String,
    pub inline_cred_name: String,
    pub inline_username: String,
    pub inline_auth_kind: proxy_model::SshAuthKind,
    pub inline_password: String,
    pub inline_private_key_path: String,
    pub inline_passphrase: String,
    pub inline_show_secret: bool,
    pub host_key_fingerprint: String,
    pub error: Option<String>,
    pub probe_note: Option<String>,
    pub probing: bool,
    pub testing: bool,
    pub test_result: Option<(bool, String)>,
    pub return_to_pf: bool,
}

impl Default for SshHostForm {
    fn default() -> Self {
        Self {
            editing_id: None,
            name: String::new(),
            host: String::new(),
            port: "22".into(),
            cred_mode: SshHostCredMode::Reuse,
            credential_id: String::new(),
            inline_cred_name: String::new(),
            inline_username: String::new(),
            inline_auth_kind: proxy_model::SshAuthKind::Password,
            inline_password: String::new(),
            inline_private_key_path: String::new(),
            inline_passphrase: String::new(),
            inline_show_secret: false,
            host_key_fingerprint: String::new(),
            error: None,
            probe_note: None,
            probing: false,
            testing: false,
            test_result: None,
            return_to_pf: false,
        }
    }
}

impl SshHostForm {
    pub fn for_new(credentials: &[proxy_model::SshCredentialSummary], return_to_pf: bool) -> Self {
        let cred_mode = if credentials.is_empty() {
            SshHostCredMode::Inline
        } else {
            SshHostCredMode::Reuse
        };
        Self {
            cred_mode,
            credential_id: credentials
                .first()
                .map(|c| c.id.clone())
                .unwrap_or_default(),
            return_to_pf,
            ..Default::default()
        }
    }
}

pub(super) enum Dialog {
    UnsavedExit(ExitKind),
    Confirm(Action),
    HistoryDiff,
    #[cfg_attr(not(any(windows, test)), allow(dead_code))]
    ExitChoice,
    Create,
    PacBatch,
    PortForward,
    SshCredential,
    SshHost,
    Share,
}
#[derive(Clone)]
pub(super) struct HistoryPreview {
    pub json: String,
    pub core: String,
}
pub(super) struct Uncertain {
    pub request_id: String,
    pub command: Command,
    pub json: Option<String>,
    pub profile: String,
    pub assets: String,
}
pub(super) struct Failure {
    error: Error,
    request: String,
}
#[derive(Clone)]
struct Event {
    level: &'static str,
    timestamp: String,
    message: String,
    request: String,
    at: Instant,
}
pub(super) struct UiState {
    pub page: Page,
    pub(super) profile_tab: ProfileTab,
    advanced_tab: AdvancedTab,
    settings_tab: SettingsTab,
    pub workspace: Workspace,
    pub connected: bool,
    pub settings_loaded: bool,
    pub profiles_loaded: bool,
    pub saved_settings: Option<Settings>,
    pub settings_dirty: bool,
    pub persistence: bool,
    pub workspace_valid: bool,
    unreadable_drafts: std::collections::HashSet<String>,
    deleted_profiles: std::collections::HashSet<String>,
    pub validation_epoch: u64,
    pub pending_auto_apply: bool,
    pub preview_open: bool,
    pub runtime: Option<RuntimeConfig>,
    pub uncertain: Option<Uncertain>,
    pub operation_details: String,
    pub(super) failure: Option<Failure>,
    pub(super) drafts: HashMap<String, Draft>,
    pub(super) profile_cache: HashMap<String, Profile>,
    draft_dirty_since: Option<Instant>,
    pub(super) number_inputs: crate::workspace::NumberInputs,
    pending_draft_writes: std::collections::HashSet<String>,
    discarded_draft_state: Option<String>,
    pub(super) draft_conflict: bool,
    pub(super) dialog: Option<Dialog>,
    new_name: String,
    new_json: String,
    import_json: bool,
    new_scene: crate::guided_config::Scene,
    new_template: crate::guided_config::Template,
    new_split_preset: bool,
    new_split_sniffing: bool,
    new_config_key: Option<(
        crate::guided_config::Scene,
        crate::guided_config::Template,
        bool,
        bool,
    )>,
    new_share_imported: bool,
    new_share_input: String,
    new_share_warnings: Vec<String>,
    new_share_error: Option<String>,
    new_editor_epoch: u64,
    new_node_names: std::collections::BTreeMap<String, String>,
    pub(super) node_share_open: bool,
    pub(super) node_share_input: String,
    pub(super) node_share_feedback: Option<String>,
    pub(super) pf_form: crate::port_forward::PortForwardForm,
    pub(super) pf_error: Option<String>,
    pub(super) pf_subtab: PortForwardSubTab,
    pub(super) ssh_state: proxy_model::SshState,
    pub(super) ssh_loaded: bool,
    pub(super) ssh_cred_form: SshCredentialForm,
    pub(super) ssh_host_form: SshHostForm,
    pub(super) ssh_host_tests: HashMap<String, SshHostTestStatus>,
    pub(super) inbounds_native: bool,
    pub(super) inline_rename: Option<String>,
    pub(super) guided: crate::guided_config::State,
    import_error: Option<String>,
    import_source: Option<PathBuf>,
    import_use_source_assets: bool,
    pub onboarding: bool,
    setup_initialized: bool,
    setup_step: usize,
    advance_setup_after_save: bool,
    pub logs_paused: bool,
    frozen_logs: Vec<(String, String)>,
    log_search: String,
    log_level: String,
    log_source: usize,
    events: Vec<Event>,
    route_port: String,
    route_ip: String,
    route_source: String,
    route_source_port: String,
    route_user: String,
    route_protocol: String,
    route_attrs: String,
    pub route_tested: Option<(String, RouteContext)>,
    pub pac_test_url: String,
    pub pac_domain_input: String,
    pub pac_domain_filter: String,
    pub pac_domain_kind_filter: PacDomainFilter,
    pub pac_selected_domains: std::collections::BTreeSet<String>,
    pub pac_batch_input: String,
    pub pac_batch_replace: bool,
    pub pac_batch_report: Option<String>,
    pub pac_geosite_search: String,
    pub pac_compile_key: Option<(String, std::net::SocketAddr, proxy_model::PacMode, u64)>,
    pub pac_compile_stats: proxy_pac::PacCompileStats,
    pub geodata_cache: Option<GeoDataCache>,
    pub restore_attempted: bool,
    pub background_hide: BackgroundHide,
    pub exit_choice: ExitKind,
    pub exit_remember: bool,
    pub pac_test: String,
    pub pac_notes: Vec<String>,
    pub pac_checked: bool,
    pub probe_adhoc_url: String,
    pub probe_config_draft: proxy_model::ProxyProbeConfig,
    pub probe_config_dirty: bool,
    pub probe_config_loaded: bool,
    pub history_preview: Option<HistoryPreview>,
    pub latency_host: String,
    pub latency_port: String,
    pub latency_result: String,
    undo: Vec<String>,
    redo: Vec<String>,
    undo_at: Option<Instant>,
    last_seen_editor: String,
    restoring_undo: bool,
    pub(super) allow_dynamic: bool,
    pub(super) share: share::ShareDialogState,
    pub(super) bundle_import: share::BundleImportState,
    pub(super) pending_followups: std::collections::VecDeque<Command>,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            page: Page::Overview,
            profile_tab: ProfileTab::Outbounds,
            advanced_tab: AdvancedTab::Inbounds,
            settings_tab: SettingsTab::Core,
            workspace: Workspace::default(),
            connected: false,
            settings_loaded: false,
            profiles_loaded: false,
            saved_settings: None,
            settings_dirty: false,
            persistence: false,
            workspace_valid: true,
            unreadable_drafts: Default::default(),
            deleted_profiles: Default::default(),
            validation_epoch: 0,
            pending_auto_apply: false,
            preview_open: false,
            runtime: None,
            uncertain: None,
            operation_details: String::new(),
            failure: None,
            drafts: HashMap::new(),
            profile_cache: HashMap::new(),
            draft_dirty_since: None,
            number_inputs: Default::default(),
            pending_draft_writes: Default::default(),
            discarded_draft_state: None,
            draft_conflict: false,
            dialog: None,
            new_name: String::new(),
            new_json: String::new(),
            import_json: false,
            new_scene: Default::default(),
            new_template: Default::default(),
            new_split_preset: true,
            new_split_sniffing: true,
            new_config_key: None,
            new_share_imported: false,
            new_share_input: String::new(),
            new_share_warnings: Vec::new(),
            new_share_error: None,
            new_editor_epoch: 0,
            new_node_names: Default::default(),
            node_share_open: false,
            node_share_input: String::new(),
            node_share_feedback: None,
            pf_form: Default::default(),
            pf_error: None,
            pf_subtab: Default::default(),
            ssh_state: Default::default(),
            ssh_loaded: false,
            ssh_cred_form: Default::default(),
            ssh_host_form: Default::default(),
            ssh_host_tests: Default::default(),
            inbounds_native: false,
            inline_rename: None,
            guided: Default::default(),
            import_error: None,
            import_source: None,
            import_use_source_assets: false,
            onboarding: true,
            setup_initialized: false,
            setup_step: 0,
            advance_setup_after_save: false,
            logs_paused: false,
            frozen_logs: vec![],
            log_search: String::new(),
            log_level: "全部".into(),
            log_source: 0,
            events: vec![],
            route_port: "443".into(),
            route_ip: String::new(),
            route_source: String::new(),
            route_source_port: String::new(),
            route_user: String::new(),
            route_protocol: String::new(),
            route_attrs: String::new(),
            route_tested: None,
            pac_test_url: "https://example.com/".into(),
            pac_domain_input: String::new(),
            pac_domain_filter: String::new(),
            pac_domain_kind_filter: PacDomainFilter::default(),
            pac_selected_domains: Default::default(),
            pac_batch_input: String::new(),
            pac_batch_replace: false,
            pac_batch_report: None,
            pac_geosite_search: String::new(),
            pac_compile_key: None,
            pac_compile_stats: Default::default(),
            geodata_cache: None,
            restore_attempted: false,
            background_hide: BackgroundHide::None,
            exit_choice: ExitKind::Gui,
            exit_remember: true,
            pac_test: String::new(),
            pac_notes: vec![],
            pac_checked: false,
            probe_adhoc_url: proxy_model::DEFAULT_PROXY_PROBE_URL.into(),
            probe_config_draft: Default::default(),
            probe_config_dirty: false,
            probe_config_loaded: false,
            history_preview: None,
            latency_host: String::new(),
            latency_port: "443".into(),
            latency_result: String::new(),
            undo: vec![],
            redo: vec![],
            undo_at: None,
            last_seen_editor: String::new(),
            restoring_undo: false,
            allow_dynamic: false,
            share: Default::default(),
            bundle_import: Default::default(),
            pending_followups: Default::default(),
        }
    }
}

pub(super) fn configure(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = if cfg!(windows) {
        let windows = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        vec![
            windows.join("Fonts/msyh.ttc"),
            windows.join("Fonts/simhei.ttf"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
        ]
    };
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("cjk".into(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push("cjk".into());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
    apply_base_style(ctx);
}

#[cfg(test)]
pub(super) fn configure_without_system_cjk(ctx: &egui::Context) {
    ctx.set_fonts(egui::FontDefinitions::default());
    apply_base_style(ctx);
}

fn apply_base_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.interact_size.y = CONTROL_HEIGHT;
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(25.0));
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        let dark = style.visuals.dark_mode;
        style.visuals.widgets.noninteractive.fg_stroke.color = if dark {
            Color32::from_rgb(231, 237, 246)
        } else {
            Color32::from_rgb(23, 32, 46)
        };
        style.visuals.weak_text_color = Some(if dark {
            Color32::from_rgb(154, 167, 186)
        } else {
            Color32::from_rgb(91, 102, 120)
        });
        style.visuals.panel_fill = if dark {
            Color32::from_rgb(15, 21, 30)
        } else {
            Color32::from_rgb(241, 244, 249)
        };
        style.visuals.window_fill = if dark {
            Color32::from_rgb(24, 32, 44)
        } else {
            Color32::WHITE
        };
        style.visuals.faint_bg_color = if dark {
            Color32::from_rgb(30, 40, 55)
        } else {
            Color32::from_rgb(248, 250, 253)
        };
        style.visuals.widgets.noninteractive.bg_stroke.color = if dark {
            Color32::from_rgb(43, 55, 73)
        } else {
            Color32::from_rgb(223, 229, 239)
        };
        style.visuals.widgets.inactive.bg_fill = if dark {
            Color32::from_rgb(24, 32, 44)
        } else {
            Color32::WHITE
        };
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(
            1.0,
            if dark {
                Color32::from_rgb(60, 76, 99)
            } else {
                Color32::from_rgb(197, 208, 224)
            },
        );
        style.visuals.selection.bg_fill = if dark {
            Color32::from_rgba_unmultiplied(77, 151, 255, 41)
        } else {
            Color32::from_rgba_unmultiplied(29, 111, 232, 32)
        };
        style.visuals.selection.stroke = egui::Stroke::new(
            1.0,
            if dark {
                Color32::from_rgb(77, 151, 255)
            } else {
                Color32::from_rgb(29, 111, 232)
            },
        );
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(8);
    });
}
fn accent(ui: &egui::Ui) -> Color32 {
    crate::widgets::accent_color(ui)
}
fn card<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_with_min_height(ui, 0.0, body)
}
fn card_with_min_height<R>(
    ui: &mut egui::Ui,
    height: f32,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::new()
        .fill(ui.visuals().window_fill())
        .stroke(egui::Stroke::new(1.0, crate::widgets::border_color(ui)))
        .corner_radius(10)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            // The requested outer height includes both one-point frame borders.
            ui.set_min_height((height - 34.0).max(0.0));
            body(ui)
        })
        .inner
}
fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    let color = accent(ui);
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).strong().color(Color32::WHITE))
            .fill(color)
            .stroke(egui::Stroke::new(1.0, color)),
    )
    .clicked()
}
fn heading(ui: &mut egui::Ui, title: &str, note: &str) {
    ui.heading(title);
    ui.label(RichText::new(note).weak());
    ui.add_space(10.0);
}
fn line(ui: &mut egui::Ui, label: &str, value: &str) {
    let label_width = 116.0_f32.min(ui.available_width() * 0.36);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_width(label_width);
                ui.add(egui::Label::new(RichText::new(label).weak()).wrap());
            },
        );
        ui.vertical(|ui| {
            ui.set_min_width(ui.available_width());
            ui.add(egui::Label::new(value).wrap());
        });
    });
}
fn path_input(ui: &mut egui::Ui, value: &mut String) -> egui::Response {
    let mut shown = display_path_text(value);
    let response = ui.add(singleline(&mut shown, f32::INFINITY));
    // Merely rendering a friendly path must not alter extended-path semantics.
    if response.changed() {
        *value = shown;
    }
    response
}
fn mode_name(mode: ProxyMode) -> &'static str {
    match mode {
        ProxyMode::Direct => "不接管系统代理",
        ProxyMode::Global => "使用本机 HTTP 代理",
        ProxyMode::Pac => "使用 PAC 自动代理",
    }
}
fn availability_tone(state: ProxyAvailabilityState) -> crate::widgets::BadgeTone {
    match state {
        ProxyAvailabilityState::Available => crate::widgets::BadgeTone::Success,
        ProxyAvailabilityState::Degraded | ProxyAvailabilityState::DirectOnly => {
            crate::widgets::BadgeTone::Warn
        }
        ProxyAvailabilityState::Unavailable => crate::widgets::BadgeTone::Danger,
        ProxyAvailabilityState::Checking => crate::widgets::BadgeTone::Accent,
        ProxyAvailabilityState::Stopped
        | ProxyAvailabilityState::Untested
        | ProxyAvailabilityState::Unsupported => crate::widgets::BadgeTone::Muted,
    }
}
pub(super) fn command_name(command: &Command) -> &'static str {
    match command {
        Command::ValidateDraft { .. } => "检查配置",
        Command::ApplyProfile { .. } => "应用配置",
        Command::CreateProfile { .. } => "创建配置",
        Command::UpdateProfile { .. } => "保存配置",
        Command::DeleteProfile { .. } => "删除配置",
        Command::StageHistory { .. } => "载入历史",
        Command::SaveSettings { .. } => "保存设置",
        Command::StopCore => "停止 Core",
        Command::RestartCore => "重启 Core",
        Command::ResolveProxyConflict => "解除代理冲突",
        Command::ResolveStartupConflict => "重置自启任务冲突",
        Command::SetProxyMode { .. } => "切换系统代理",
        Command::Shutdown => "退出程序",
        Command::TestRoute { .. } | Command::TestRunningRoute { .. } => "路由静态预测",
        Command::TestPac { .. } => "PAC 执行测试",
        Command::TestLatency { .. } => "TCP 延迟探测",
        Command::GetOperation { .. } | Command::GetOperationDetails { .. } => "查询操作结果",
        Command::GetSshState => "读取 SSH 隧道状态",
        Command::ProbeSshHostKey { .. } => "探测 SSH 服务器指纹",
        Command::TestSshHost { .. } => "测试 SSH 跳板机连接",
        Command::SaveSshCredential { .. } => "保存 SSH 凭证",
        Command::DeleteSshCredential { .. } => "删除 SSH 凭证",
        Command::SaveSshHost { .. } => "保存 SSH 跳板机",
        Command::DeleteSshHost { .. } => "删除 SSH 跳板机",
        Command::SaveSshForward { .. } => "保存 SSH 端口转发",
        Command::DeleteSshForward { .. } => "删除 SSH 端口转发",
        Command::StartSshForward { .. } => "启动 SSH 端口转发",
        Command::StopSshForward { .. } => "停止 SSH 端口转发",
        Command::StartAllSshForwards => "批量启动 SSH 隧道",
        Command::StopAllSshForwards => "批量停止 SSH 隧道",
        Command::CheckProxyAvailability { .. } => "检测代理可用性",
        Command::SaveProxyProbeConfig { .. } => "保存代理探测设置",
        _ => "读取后台状态",
    }
}
fn error_advice(code: &str) -> &'static str {
    error_advice_for(code, true)
}
fn error_advice_for(code: &str, is_write: bool) -> &'static str {
    match code {
        "CONNECTION_LOST" | "IPC_TIMEOUT" => {
            if is_write {
                "后台响应中断，请先查询原操作结果，再进行其他变更。"
            } else {
                "后台服务尚未就绪或连接已断开，请稍候或点击刷新状态。"
            }
        }
        "RECOVERY_REQUIRED" | "OPERATION_RECOVERY_REQUIRED" | "PROXY_RECOVERY_REQUIRED" => {
            "存在未完成的恢复，请检查状态和证据；不要覆盖其他程序的设置。"
        }
        "PROXY_CONFLICT" | "PROXY_OWNERSHIP_CONFLICT" | "PROXY_LEASE_CONFLICT" => {
            "系统代理已被其他程序或系统设置修改。请检查差异后选择“保留当前系统代理并解除冲突”，不会覆盖当前外部设置。"
        }
        "STARTUP_CONFLICT" => {
            "登录自启计划任务已被外部修改或与所有权记录不一致。请查看日志中的任务名称，或选择“重新接管自启计划任务”按当前设置重建并同步。"
        }
        "GENERATION_CONFLICT" | "REVISION_CONFLICT" => {
            "运行版本或已保存配置已变化，请刷新状态并重新检查。"
        }
        "HTTP_READY_FAILED" | "LOCAL_PROXY_NOT_READY" | "CORE_START_FAILED" => {
            "Core 或本机入口未就绪，请检查端口占用与 Core 日志，修正后重新检查。"
        }
        "PROXY_ENDPOINT_INVALID" => {
            "接管需要无认证、固定数值端口的回环或 0.0.0.0 监听 HTTP/Mixed 入口，请检查运行配置及入站 Tag。"
        }
        "INVALID_CORE_PATH" | "INVALID_CORE" | "CORE_INTEGRITY_FAILED" => {
            "Core 路径或文件校验失败，请重新选择或修复官方依赖，再检查配置。"
        }
        "CORE_VALIDATION_FAILED" | "CONFIG_PARSE_FAILED" | "STATIC_VALIDATION_FAILED" => {
            "配置检查未通过，请修正 JSON 或资源路径后重试（可在「日志」页查看 Core 返回的详细报错，如缺少 geoip/geosite 文件或标签不存在）。"
        }
        "DRAFT_SAVE_FAILED" | "DRAFT_RECOVERY_REQUIRED" => {
            "草稿尚未写入磁盘，修改仍保留在界面中。请检查数据目录权限，或先导出草稿。"
        }
        "PROFILE_IN_USE" => "该配置仍由上次应用版本引用，请先应用另一份配置后再删除。",
        "DEPENDENCY_INSTALL_FAILED" => {
            "依赖安装失败，请检查网络和官方版本号；已有运行版本不会自动替换。"
        }
        _ => "操作未能完成，请查看详情与日志，修正原因后再继续。",
    }
}

impl App {
    pub(super) fn is_running(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|s| s.core_state == "Running")
    }
    pub(super) fn committed_id(&self) -> Option<&str> {
        self.status
            .as_ref()
            .and_then(|s| s.generation.as_ref())
            .map(|g| g.candidate.profile_id.as_str())
    }
    pub(super) fn connection_restored(&mut self) {
        if !self.ui.connected {
            self.add_event("信息", "后台已连接", String::new());
            if self.ui.uncertain.is_none() {
                self.message.clear();
            }
        }
        self.ui.connected = true;
        if self.ui.uncertain.is_none()
            && self
                .ui
                .failure
                .as_ref()
                .is_some_and(|f| f.error.code == "CONNECTION_LOST")
        {
            self.ui.failure = None;
        }
    }
    pub(super) fn has_draft_conflict(&self) -> bool {
        self.ui.draft_conflict
    }
    pub(super) fn write_blocker(&self) -> Option<&'static str> {
        if !self.ui.connected {
            Some("后台未连接，当前展示最后已知状态")
        } else if self.ui.uncertain.is_some() {
            Some("上次操作结果待确认，请先查询")
        } else if self.busy {
            Some("正在执行操作，请稍候")
        } else if self.shutdown_requested {
            Some("正在等待停止并退出")
        } else if self.status.as_ref().is_some_and(|s| s.recovery_required) {
            Some("恢复尚未完成，普通变更已暂停")
        } else {
            None
        }
    }

    pub(super) fn can_resolve_proxy_conflict(&self) -> bool {
        self.ui.connected
            && !self.busy
            && self.ui.uncertain.is_none()
            && !self.shutdown_requested
            && self.status.as_ref().is_some_and(|s| {
                s.recovery_required
                    && s.proxy_state == "Conflict"
                    && !s.last_error.as_ref().is_some_and(|e| {
                        matches!(
                            e.code.as_str(),
                            "PROXY_RECOVERY_REQUIRED"
                                | "OPERATION_RECOVERY_REQUIRED"
                                | "STARTUP_CONFLICT"
                                | "SETTINGS_CONFLICT"
                        )
                    })
            })
    }

    pub(super) fn can_resolve_startup_conflict(&self) -> bool {
        self.ui.connected
            && !self.busy
            && self.ui.uncertain.is_none()
            && !self.shutdown_requested
            && (self.status.as_ref().is_some_and(|s| {
                s.recovery_required
                    && s.last_error
                        .as_ref()
                        .is_some_and(|e| e.code == "STARTUP_CONFLICT")
            }) || self
                .ui
                .failure
                .as_ref()
                .is_some_and(|f| f.error.code == "STARTUP_CONFLICT"))
    }

    pub(super) fn can_save_settings(&self) -> bool {
        self.ui.connected
            && !self.busy
            && self.ui.uncertain.is_none()
            && !self.shutdown_requested
            && !self.status.as_ref().is_some_and(|s| {
                s.recovery_required
                    && s.last_error.as_ref().is_some_and(|e| {
                        matches!(
                            e.code.as_str(),
                            "STARTUP_CONFLICT"
                                | "SETTINGS_CONFLICT"
                                | "INSTALLATION_ID_MISSING"
                                | "OPERATION_RECOVERY_REQUIRED"
                        )
                    })
            })
    }
    pub(super) fn add_event(
        &mut self,
        level: &'static str,
        message: impl Into<String>,
        request: impl Into<String>,
    ) {
        self.ui.events.insert(
            0,
            Event {
                level,
                timestamp: proxy_runtime::local_timestamp(),
                message: message.into(),
                request: request.into(),
                at: Instant::now(),
            },
        );
        self.ui.events.truncate(300);
    }
    pub(super) fn fail(&mut self, error: Error, request: String) {
        let advice = error_advice(&error.code);
        self.fail_with_advice(error, request, advice);
    }
    pub(super) fn fail_command(&mut self, command: &Command, error: Error, request: String) {
        let advice = error_advice_for(&error.code, command.is_write());
        self.fail_with_advice(error, request, advice);
    }
    fn fail_with_advice(&mut self, error: Error, request: String, advice: &str) {
        self.message = advice.into();
        self.add_event(
            "错误",
            format!("{} · {}", self.message, error),
            request.clone(),
        );
        self.ui.failure = Some(Failure { error, request });
    }
    pub(super) fn clear_request_failure(&mut self) {
        // A successful backend request cannot repair local draft/settings IO.
        // Events and the backend operation record retain the failed request.
        if self
            .ui
            .failure
            .as_ref()
            .is_some_and(|failure| !failure.request.is_empty())
        {
            self.ui.failure = None;
        }
        self.failed_request = None;
    }
    pub(super) fn accept_saved_settings(&mut self, settings: Settings, assets: String) {
        let portable_core = to_portable_string(&self.root, &settings.core_path.to_string_lossy());
        let portable_assets = to_portable_string(&self.root, &assets);
        self.ui
            .workspace
            .asset_roots
            .insert(portable_core.clone(), portable_assets.clone());
        self.ui.settings_loaded = true;
        self.ui.settings_dirty = to_portable_string(&self.root, &self.core_path) != portable_core
            || to_portable_string(&self.root, &self.asset_root) != portable_assets
            || self.settings.start_on_logon != settings.start_on_logon
            || self.settings.connect_on_start != settings.connect_on_start
            || self.settings.restore_last_state != settings.restore_last_state;
        self.finish_setup_save(&settings);
        self.ui.saved_settings = Some(settings);
        self.save_workspace();
    }
    pub(super) fn save_workspace(&mut self) {
        if !self.ui.workspace_valid {
            self.message =
                "原界面资料无法读取，未覆盖 gui.json；名称暂保留在当前会话，请先备份并修复该文件。"
                    .into();
            return;
        }
        if self.ui.persistence
            && let Err(error) = self.ui.workspace.save(&self.root)
        {
            self.fail(error, String::new());
        }
    }
    pub(super) fn has_draft_edits(&self) -> bool {
        self.editor != self.saved_json || !self.ui.number_inputs.is_empty()
    }
    pub(super) fn draft_edited(&mut self) {
        if !self.ui.restoring_undo {
            let now = Instant::now();
            let new_group = self
                .ui
                .undo_at
                .is_none_or(|at| now.duration_since(at) > Duration::from_millis(600));
            if new_group && self.editor != self.ui.last_seen_editor {
                self.ui.undo.push(self.ui.last_seen_editor.clone());
                if self.ui.undo.len() > 50 {
                    self.ui.undo.remove(0);
                }
                self.ui.redo.clear();
            }
            self.ui.undo_at = Some(now);
            self.ui.last_seen_editor = self.editor.clone();
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&self.editor) {
            self.ui
                .number_inputs
                .retain(|path, input| value.pointer(path).is_some_and(|n| input.matches(n)));
        }
        self.invalidate_candidate();
        self.ui.draft_dirty_since = Some(Instant::now());
    }
    pub(super) fn reset_undo(&mut self, editor: &str) {
        self.ui.undo.clear();
        self.ui.redo.clear();
        self.ui.undo_at = None;
        self.ui.last_seen_editor = editor.into();
    }
    pub(super) fn undo_edit(&mut self) {
        if let Some(previous) = self.ui.undo.pop() {
            let current = std::mem::replace(&mut self.editor, previous);
            self.ui.redo.push(current);
            self.ui.restoring_undo = true;
            self.draft_edited();
            self.ui.restoring_undo = false;
            self.ui.last_seen_editor = self.editor.clone();
            self.ui.undo_at = None;
            self.message = "已撤销上一步编辑。".into();
        }
    }
    pub(super) fn redo_edit(&mut self) {
        if let Some(next) = self.ui.redo.pop() {
            let current = std::mem::replace(&mut self.editor, next);
            self.ui.undo.push(current);
            self.ui.restoring_undo = true;
            self.draft_edited();
            self.ui.restoring_undo = false;
            self.ui.last_seen_editor = self.editor.clone();
            self.ui.undo_at = None;
            self.message = "已重做编辑。".into();
        }
    }
    /// Snapshot the visible editor and retry every pending write, including other profiles.
    pub(super) fn stash_draft(&mut self) -> bool {
        if !self.profile_id.is_empty() && !self.ui.deleted_profiles.contains(&self.profile_id) {
            let dirty = self.has_draft_edits() || self.ui.draft_conflict;
            if dirty {
                let base_revision = if self.ui.draft_conflict {
                    self.ui
                        .drafts
                        .get(&self.profile_id)
                        .map(|d| d.base_revision.clone())
                        .unwrap_or_else(|| self.revision.clone())
                } else {
                    self.revision.clone()
                };
                self.ui.drafts.insert(
                    self.profile_id.clone(),
                    Draft {
                        number_inputs: self.ui.number_inputs.clone(),
                        base_revision,
                        json: self.editor.clone(),
                    },
                );
                self.ui.pending_draft_writes.insert(self.profile_id.clone());
            } else if !self.ui.unreadable_drafts.contains(&self.profile_id) {
                self.ui.drafts.remove(&self.profile_id);
                self.ui.pending_draft_writes.insert(self.profile_id.clone());
            }
        }
        self.flush_pending_drafts()
    }
    fn flush_pending_drafts(&mut self) -> bool {
        let pending: Vec<_> = self.ui.pending_draft_writes.iter().cloned().collect();
        for profile in pending {
            let result = if !self.ui.persistence {
                Ok(())
            } else if self.ui.unreadable_drafts.contains(&profile) {
                Err(Error::new(
                    "DRAFT_RECOVERY_REQUIRED",
                    "原草稿无法读取，请先导出当前修改，或明确选择保留 / 放弃草稿。",
                ))
            } else if let Some(draft) = self.ui.drafts.get(&profile) {
                draft.save(&self.root, &profile)
            } else {
                Draft::remove(&self.root, &profile)
            };
            match result {
                Ok(()) => {
                    self.ui.pending_draft_writes.remove(&profile);
                }
                Err(error) => self.fail(
                    Error::new(
                        "DRAFT_SAVE_FAILED",
                        format!(
                            "配置 {} 的草稿未保存：{error}",
                            self.ui.workspace.name(&profile)
                        ),
                    ),
                    String::new(),
                ),
            }
        }
        let saved = self.ui.pending_draft_writes.is_empty();
        if saved
            && self
                .ui
                .failure
                .as_ref()
                .is_some_and(|f| f.error.code == "DRAFT_SAVE_FAILED")
        {
            self.ui.failure = None;
            self.message.clear();
        }
        if saved && matches!(self.ui.dialog, Some(Dialog::UnsavedExit(_))) {
            self.ui.dialog = None;
        }
        self.ui.draft_dirty_since = if saved { None } else { Some(Instant::now()) };
        saved
    }
    fn draft_state(&self) -> String {
        let drafts: std::collections::BTreeMap<_, _> = self.ui.drafts.iter().collect();
        hash(
            &serde_json::to_vec(&(
                &self.profile_id,
                &self.editor,
                &self.revision,
                &self.ui.number_inputs,
                drafts,
            ))
            .unwrap(),
        )
    }
    pub(super) fn open_history_diff(&mut self, preview: HistoryPreview) {
        self.ui.history_preview = Some(preview);
        self.ui.dialog = Some(Dialog::HistoryDiff);
    }
    pub(super) fn load_history_preview(&mut self) -> bool {
        let Some(preview) = self.ui.history_preview.take() else {
            return false;
        };
        self.editor = preview.json;
        self.ui.number_inputs.clear();
        self.invalidate_candidate();
        self.draft_edited();
        self.stash_draft();
        self.message = "历史版本已载入为草稿；请重新检查并预览。".into();
        true
    }
    #[cfg_attr(not(any(windows, test)), allow(dead_code))]
    pub(super) fn open_exit_dialog(&mut self) {
        self.ui.exit_choice = match self.ui.workspace.exit_default {
            crate::workspace::ExitDefault::Gui => ExitKind::Gui,
            crate::workspace::ExitDefault::Shutdown => ExitKind::Shutdown,
        };
        self.ui.exit_remember = true;
        self.ui.dialog = Some(Dialog::ExitChoice);
    }
    pub(super) fn request_exit(&mut self, kind: ExitKind) {
        if self.exit
            || (matches!(kind, ExitKind::Gui)
                && (self.busy || self.shutdown_requested || self.ui.uncertain.is_some()))
        {
            return;
        }
        if matches!(kind, ExitKind::Shutdown) {
            if let Some(running) = self.status.as_ref().map(|s| s.core_state == "Running") {
                self.sync_restore_state(running, true);
            }
            self.shutdown_requested = true;
        } else if self.drafts_ready_for_exit(kind) {
            self.exit = true;
        }
    }
    pub(super) fn drafts_ready_for_exit(&mut self, kind: ExitKind) -> bool {
        if self.ui.discarded_draft_state.as_ref() == Some(&self.draft_state()) || self.stash_draft()
        {
            return true;
        }
        self.shutdown_requested = false;
        self.ui.dialog = Some(Dialog::UnsavedExit(kind));
        self.message = "草稿未保存，已取消退出。请重试保存、导出或明确放弃未保存修改。".into();
        if let Some(wakeup) = &self.wakeup {
            wakeup.reveal();
        }
        false
    }
    fn discard_unsaved_and_exit(&mut self, kind: ExitKind) {
        // Consent only applies to this exact editor/draft state. Later edits need a new save gate.
        self.ui.discarded_draft_state = Some(self.draft_state());
        self.request_exit(kind);
    }
    fn unsaved_drafts_json(&self) -> serde_json::Value {
        let drafts: std::collections::BTreeMap<_, _> = self
            .ui
            .pending_draft_writes
            .iter()
            .map(|profile| (profile, self.ui.drafts.get(profile)))
            .collect();
        serde_json::json!({"format": "proxy-pilot-unsaved-drafts-v1", "drafts": drafts})
    }
    #[cfg(any(windows, test))]
    fn export_unsaved_drafts(&mut self, destination: &std::path::Path) -> bool {
        match proxy_config::write_json(destination, &self.unsaved_drafts_json()) {
            Ok(()) => {
                self.message = format!(
                    "未保存草稿已导出到 {}；可返回编辑或重试保存。",
                    display_path(destination)
                );
                true
            }
            Err(error) => {
                self.fail(error, String::new());
                false
            }
        }
    }
    pub(super) fn can_auto_save_profile(&self) -> bool {
        !self.profile_id.is_empty()
            && !self.ui.deleted_profiles.contains(&self.profile_id)
            && self.selected_profile == self.profile_id
            && self.editor != self.saved_json
            && self.ui.number_inputs.is_empty()
            && !self.ui.draft_conflict
            && self.write_blocker().is_none()
            && proxy_config::parse_document(self.editor.as_bytes()).is_ok()
    }
    pub(super) fn auto_save_profile(&mut self) {
        if !self.can_auto_save_profile() {
            return;
        }
        self.send(Command::UpdateProfile {
            profile_id: self.profile_id.clone(),
            expected_revision: self.revision.clone(),
            json: self.editor.clone(),
        });
    }
    pub(super) fn profile_updated(&mut self, profile: Profile) {
        self.ui
            .profile_cache
            .insert(profile.id.clone(), profile.clone());
        if self.profile_id == profile.id {
            self.saved_json = profile.json.clone();
            self.revision = profile.revision.clone();
            self.ui.draft_conflict = false;
            if self.editor == self.saved_json && self.ui.number_inputs.is_empty() {
                self.ui.drafts.remove(&profile.id);
                self.ui.pending_draft_writes.remove(&profile.id);
                self.ui.draft_dirty_since = None;
                if self.ui.persistence
                    && let Err(error) = Draft::remove(&self.root, &profile.id)
                {
                    self.fail(error, String::new());
                }
            } else {
                self.ui.draft_dirty_since = Some(Instant::now());
            }
        }
    }
    pub(super) fn has_unapplied_runtime_diff(&self) -> bool {
        if self.committed_id() != Some(self.profile_id.as_str()) {
            return false;
        }
        let Some(generation) = self.status.as_ref().and_then(|s| s.generation.as_ref()) else {
            return false;
        };
        let Some(runtime) = self
            .ui
            .runtime
            .as_ref()
            .filter(|r| r.generation_id == generation.id)
        else {
            return false;
        };
        self.editor != runtime.json
    }
    pub(super) fn revert_to_running_config(&mut self) {
        let Some(generation) = self.status.as_ref().and_then(|s| s.generation.as_ref()) else {
            return;
        };
        if generation.candidate.profile_id != self.profile_id {
            return;
        }
        let Some(runtime) = self
            .ui
            .runtime
            .as_ref()
            .filter(|r| r.generation_id == generation.id)
        else {
            return;
        };
        self.editor = runtime.json.clone();
        self.ui.number_inputs.clear();
        self.ui.draft_conflict = false;
        self.draft_edited();
        self.message = "已恢复到当前运行版本的配置内容。".into();
    }
    pub(super) fn compatible_http_listeners(&self) -> Vec<(String, String)> {
        serde_json::from_str::<serde_json::Value>(&self.editor)
            .ok()
            .and_then(|v| v.get("inbounds").and_then(|a| a.as_array()).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|v| matches!(v["protocol"].as_str(), Some("http" | "mixed")))
            .filter_map(|v| {
                let tag = v["tag"].as_str()?.to_owned();
                Some((
                    tag.clone(),
                    format!(
                        "{}:{} · {}",
                        v["listen"].as_str().unwrap_or("默认监听"),
                        v.get("port")
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "待补充端口".into()),
                        tag
                    ),
                ))
            })
            .collect()
    }
    pub(super) fn sync_http_tag(&mut self) {
        let listeners = self.compatible_http_listeners();
        if let Some((first, _)) = listeners.first()
            && !listeners.iter().any(|(tag, _)| tag == &self.http_tag)
        {
            self.http_tag = first.clone();
        }
    }
    pub(super) fn flush_draft_if_due(&mut self) {
        if self
            .ui
            .draft_dirty_since
            .is_some_and(|since| since.elapsed() > Duration::from_millis(800))
        {
            if self.can_auto_save_profile() {
                self.auto_save_profile();
            } else {
                self.stash_draft();
            }
        }
    }
    pub(super) fn restore_draft(&mut self) {
        self.ui.number_inputs.clear();
        self.ui.deleted_profiles.remove(&self.profile_id);
        self.ui.profile_cache.insert(
            self.profile_id.clone(),
            Profile {
                id: self.profile_id.clone(),
                json: self.saved_json.clone(),
                revision: self.revision.clone(),
            },
        );
        if !self.ui.drafts.contains_key(&self.profile_id) && self.ui.persistence {
            match Draft::load(&self.root, &self.profile_id) {
                Ok(Some(draft)) => {
                    self.ui.drafts.insert(self.profile_id.clone(), draft);
                }
                Ok(None) => {}
                Err(error) => {
                    self.ui.unreadable_drafts.insert(self.profile_id.clone());
                    self.fail(error, String::new());
                }
            }
        }
        self.ui.draft_conflict = false;
        if let Some(draft) = self.ui.drafts.get(&self.profile_id) {
            if draft.json != self.saved_json || !draft.number_inputs.is_empty() {
                self.editor = draft.json.clone();
                self.ui.number_inputs = draft.number_inputs.clone();
                self.ui.draft_conflict = draft.base_revision != self.revision;
                if !self.ui.draft_conflict && self.ui.number_inputs.is_empty() {
                    self.ui.draft_dirty_since = Some(Instant::now());
                }
            } else {
                self.ui.drafts.remove(&self.profile_id);
            }
        }
    }
    pub(super) fn switch_profile(&mut self, id: String) {
        self.stash_draft();
        if !self.ui.connected {
            if let Some(profile) = self.ui.profile_cache.get(&id).cloned() {
                self.load(profile);
            } else {
                self.message = "此配置尚未缓存，请重新连接后台后读取。".into();
            }
        } else {
            self.send(Command::GetProfile { profile_id: id });
        }
    }
    pub(super) fn profile_deleted(&mut self, profile: &str) {
        self.ui.deleted_profiles.insert(profile.into());
        self.ui.drafts.remove(profile);
        self.ui.profile_cache.remove(profile);
        self.ui.workspace.names.remove(profile);
        self.ui.workspace.node_names.remove(profile);
        self.ui.workspace.port_forward_names.remove(profile);
        self.ui.workspace.paused_port_forwards.remove(profile);
        self.ui.workspace.pac.remove(profile);
        self.ui.workspace.proxy_modes.remove(profile);
        self.save_workspace();
        if self.ui.persistence
            && let Err(error) = Draft::remove(&self.root, profile)
        {
            self.fail(error, String::new());
        }
        if self.profile_id == profile && self.selected_profile == profile {
            self.profile_id.clear();
            self.selected_profile.clear();
            self.saved_json.clear();
            self.editor.clear();
            self.ui.number_inputs.clear();
            self.revision.clear();
            self.view_epoch = self.view_epoch.wrapping_add(1);
            self.invalidate_candidate();
        }
        self.refresh_profiles();
    }
    pub(super) fn applied_draft(&mut self, profile_id: &str, json: Option<&str>) {
        let Some(json) = json else {
            return;
        };
        let revision = hash(json.as_bytes());
        if self.profile_id == profile_id {
            self.saved_json = json.into();
            self.revision = revision.clone();
            self.ui.draft_conflict = false;
            self.stash_draft();
        } else if let Some(draft) = self.ui.drafts.get_mut(profile_id) {
            if draft.json == json && draft.number_inputs.is_empty() {
                self.ui.drafts.remove(profile_id);
            } else {
                draft.base_revision = revision;
            }
            self.ui.pending_draft_writes.insert(profile_id.into());
            self.flush_pending_drafts();
        }
        self.ui.profile_cache.insert(
            profile_id.into(),
            Profile {
                id: profile_id.into(),
                json: json.into(),
                revision: hash(json.as_bytes()),
            },
        );
    }
    #[cfg(any(windows, test))]
    pub(super) fn open_profile_from_tray(&mut self, profile: &str, ctx: &egui::Context) {
        if self.busy
            || self.shutdown_requested
            || self.ui.uncertain.is_some()
            || !self.profiles.iter().any(|p| p.id == profile)
        {
            return;
        }
        self.ui.page = Page::Profiles;
        self.reveal_window(ctx);
        if self.selected_profile == profile {
            return;
        }
        if self.ui.draft_conflict || self.ui.unreadable_drafts.contains(&self.profile_id) {
            self.ui.dialog = Some(Dialog::Confirm(Action::Switch(profile.into())));
        } else {
            self.switch_profile(profile.into());
        }
    }
    pub(super) fn begin_create(&mut self, import: bool, duplicate: bool) {
        self.ui.new_name = if duplicate {
            format!("{} 副本", self.ui.workspace.name(&self.profile_id))
        } else {
            String::new()
        };
        self.ui.import_json = import || duplicate;
        self.ui.new_scene = Default::default();
        self.ui.new_template = Default::default();
        self.ui.new_split_preset = true;
        self.ui.new_split_sniffing = true;
        self.ui.new_config_key = None;
        self.ui.new_share_imported = false;
        self.ui.new_share_input.clear();
        self.ui.new_share_warnings.clear();
        self.ui.new_share_error = None;
        self.ui.new_editor_epoch = self.ui.new_editor_epoch.wrapping_add(1);
        if duplicate {
            self.ui.new_json = self.editor.clone();
            self.ui.new_node_names = self
                .ui
                .workspace
                .node_names
                .get(&self.profile_id)
                .cloned()
                .unwrap_or_default();
        } else if import {
            self.ui.new_json = default_config();
            self.ui.new_node_names = Default::default();
        } else {
            self.ui.new_config_key = Some((
                self.ui.new_scene,
                self.ui.new_template,
                self.ui.new_split_preset,
                self.ui.new_split_sniffing,
            ));
            self.ui.new_json =
                serde_json::to_string_pretty(&crate::guided_config::scene_with_options(
                    self.ui.new_scene,
                    self.ui.new_template,
                    self.ui.new_split_preset,
                    self.ui.new_split_sniffing,
                ))
                .unwrap();
            self.ui.new_node_names = std::collections::BTreeMap::from([(
                "node-1".into(),
                format!("{} 节点", self.ui.new_template.label()),
            )]);
        }
        self.ui.import_error = None;
        self.ui.import_source = None;
        self.ui.import_use_source_assets = false;
        self.ui.bundle_import = Default::default();
        self.ui.dialog = Some(Dialog::Create);
    }
    pub(super) fn imported_profile(&mut self, profile: Profile) {
        let needs_review = proxy_config::parse_document(profile.json.as_bytes())
            .is_ok_and(|value| !proxy_config::inspect_config(&value).is_empty());
        if self
            .ui
            .failure
            .as_ref()
            .is_some_and(|f| f.error.code == "STATIC_VALIDATION_FAILED")
        {
            self.ui.failure = None;
        }
        self.load(profile);
        self.clear_request_failure();
        if needs_review {
            self.ui.page = Page::Profiles;
            self.ui.profile_tab = ProfileTab::Outbounds;
            self.message = "配置原文已保留，请按字段提示补充 / 核对后检查；尚未应用。".into();
        } else {
            self.message = "配置已添加，尚未应用".into();
        }
    }
    pub(super) fn validate_draft(&mut self) {
        self.validate_draft_with_mode(false);
    }
    pub(super) fn validate_and_apply_draft(&mut self) {
        self.validate_draft_with_mode(true);
    }
    fn validate_draft_with_mode(&mut self, auto_apply: bool) {
        if !self.ui.number_inputs.is_empty() {
            self.message = "有未完成的数值输入，请在高级字段中补全或明确放弃草稿后再检查。".into();
            return;
        }
        if self.ui.draft_conflict {
            self.message = "本地修改与磁盘最新版本存在冲突，请先确认处理方式。".into();
            return;
        }
        if matches!(self.mode, ProxyMode::Global | ProxyMode::Pac) {
            self.sync_http_tag();
        }
        self.ui.pending_auto_apply = auto_apply;
        self.send(Command::ValidateDraft {
            profile_id: self.profile_id.clone(),
            expected_revision: self.revision.clone(),
            json: self.editor.clone(),
            core_path: self.core_path.clone().into(),
            asset_root: self.asset_root.clone().into(),
            allow_dynamic: self.ui.allow_dynamic,
        });
    }
    pub(super) fn finish_setup_save(&mut self, settings: &Settings) {
        if self.ui.advance_setup_after_save {
            self.ui.advance_setup_after_save = false;
            if to_portable_string(&self.root, &settings.core_path.to_string_lossy())
                == to_portable_string(&self.root, &self.core_path)
                && !self.core_path.is_empty()
            {
                self.ui.setup_step = 1;
            }
        }
    }
    pub(super) fn resolve_operation(&mut self, request_id: &str, value: serde_json::Value) {
        self.ui.operation_details = serde_json::to_string_pretty(&value).unwrap_or_default();
        let Some(uncertain) = &self.ui.uncertain else {
            self.message = "已读取操作记录。".into();
            return;
        };
        if uncertain.request_id != request_id {
            return;
        }
        if value.is_null() {
            self.message = "后台尚无可确认的终态记录，请保留请求身份并继续查询。".into();
            return;
        }
        let identity = value
            .get("request")
            .and_then(|r| r.get("request_id"))
            .or_else(|| value.get("request_id"))
            .and_then(|v| v.as_str());
        if identity != Some(request_id) {
            self.message = "返回记录与原请求身份不一致，结果仍待确认。".into();
            return;
        }
        let response = value
            .get("response")
            .cloned()
            .and_then(|v| serde_json::from_value::<Response>(v).ok());
        let summary_state = value.get("state").and_then(|v| v.as_str());
        if response
            .as_ref()
            .is_some_and(|r| r.request_id != request_id)
        {
            self.message = "响应身份不一致，原操作结果仍待确认。".into();
            return;
        }
        let expected = serde_json::to_value(&uncertain.command).unwrap_or_default();
        if value
            .pointer("/request/command")
            .is_some_and(|command| command != &expected)
            || value
                .get("command")
                .is_some_and(|command| Some(command) != expected.get("operation"))
        {
            self.message = "操作内容与原请求不一致，结果仍待确认。".into();
            return;
        }
        let terminal = response.is_some() || matches!(summary_state, Some("Succeeded" | "Failed"));
        if !terminal {
            self.message = "原操作尚未形成终态，请检查恢复状态后继续查询。".into();
            self.send(Command::GetStatus);
            return;
        }
        let uncertain = self.ui.uncertain.take().unwrap();
        let success = response.as_ref().is_some_and(|r| r.result.is_ok())
            || summary_state == Some("Succeeded");
        self.failed_request = None;
        if success {
            self.clear_request_failure();
            match uncertain.command {
                Command::ApplyProfile {
                    proxy_mode, pac, ..
                } => {
                    // The captured JSON, rather than today's selected editor, was applied.
                    self.applied_draft(&uncertain.profile, uncertain.json.as_deref());
                    self.save_profile_proxy_mode(&uncertain.profile, proxy_mode);
                    self.note_applied_proxy(proxy_mode, pac.as_ref());
                    self.invalidate_candidate();
                    self.ui.onboarding = false;
                    self.refresh_profiles();
                    if self.selected_profile == uncertain.profile {
                        self.send(Command::GetProfile {
                            profile_id: uncertain.profile,
                        });
                    }
                }
                Command::SetProxyMode { mode, pac, .. } => {
                    if let Some(running_profile) = self.committed_id().map(str::to_owned) {
                        self.save_profile_proxy_mode(&running_profile, mode);
                        if self.profile_id == running_profile {
                            self.mode = mode;
                        }
                    }
                    self.note_applied_proxy(mode, pac.as_ref());
                    self.invalidate_candidate();
                    self.refresh_profiles();
                }
                Command::CreateProfile { profile_id, .. } => {
                    self.send(Command::GetProfile { profile_id });
                    self.refresh_profiles();
                }
                Command::DeleteProfile { profile_id, .. } => self.profile_deleted(&profile_id),
                Command::SaveSettings { settings } => {
                    self.accept_saved_settings(settings, uncertain.assets);
                }
                Command::Shutdown => {
                    self.shutdown_requested = false;
                    self.request_exit(ExitKind::Gui);
                }
                _ => {
                    self.invalidate_candidate();
                    self.refresh_profiles();
                }
            }
            self.message = "已确认原操作成功，正在读取最新状态。".into();
            self.add_event("信息", "原请求查询确认成功", request_id);
        } else {
            let error = response
                .and_then(|r| r.result.err())
                .or_else(|| {
                    value
                        .get("error")
                        .cloned()
                        .and_then(|v| serde_json::from_value(v).ok())
                })
                .unwrap_or_else(|| Error::new("OPERATION_FAILED", "The original operation failed"));
            self.fail(error, request_id.into());
            self.failed_request = Some(request_id.into());
        }
        self.send(Command::GetStatus);
    }

    pub(super) const CORE_LOG_LEVELS: [(&'static str, &'static str); 5] = [
        ("warning", "warning（警告 · 默认）"),
        ("info", "info（信息 · 记录连接）"),
        ("debug", "debug（调试 · 最详细）"),
        ("error", "error（仅错误）"),
        ("none", "none（关闭日志）"),
    ];

    pub(super) fn core_log_level(&self) -> String {
        serde_json::from_str::<serde_json::Value>(&self.editor)
            .ok()
            .and_then(|v| {
                v.pointer("/log/loglevel")
                    .and_then(|s| s.as_str())
                    .map(|s| s.trim().to_ascii_lowercase())
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "warning".into())
    }

    pub(super) fn running_core_log_level(&self) -> Option<String> {
        let g = self.status.as_ref().and_then(|s| s.generation.as_ref())?;
        let runtime = self
            .ui
            .runtime
            .as_ref()
            .filter(|r| r.generation_id == g.id)?;
        Some(
            serde_json::from_str::<serde_json::Value>(&runtime.json)
                .ok()
                .and_then(|v| {
                    v.pointer("/log/loglevel")
                        .and_then(|s| s.as_str())
                        .map(|s| s.trim().to_ascii_lowercase())
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "warning".into()),
        )
    }

    pub(super) fn set_core_log_level(&mut self, level: &str) -> bool {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&self.editor) else {
            self.message =
                "当前配置 JSON 语法有误，请先在「更多与源码 → 原始 JSON」中修正。".into();
            return false;
        };
        if let Err(err) =
            crate::guided_config::patch(&mut value, "/log/loglevel", serde_json::json!(level))
        {
            self.message = err.into();
            return false;
        }
        self.editor = serde_json::to_string_pretty(&value).unwrap();
        self.draft_edited();
        true
    }

    pub(super) fn core_log_level_picker(
        &mut self,
        ui: &mut egui::Ui,
        enabled: bool,
        id_salt: &str,
    ) {
        let current = self.core_log_level();
        let selected_label = Self::CORE_LOG_LEVELS
            .iter()
            .find(|(v, _)| *v == current)
            .map(|(_, label)| *label)
            .unwrap_or(current.as_str());
        let mut chosen = current.clone();
        ui.add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt(id_salt)
                .width(190.0)
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    for (value, label) in Self::CORE_LOG_LEVELS {
                        ui.selectable_value(&mut chosen, value.into(), label);
                    }
                });
        });
        if chosen != current && self.set_core_log_level(&chosen) {
            self.message = format!("已将 Core 日志级别设为 {chosen}（应用后生效）。");
        }
    }

    pub(super) fn pac_mode_label(mode: PacMode) -> &'static str {
        match mode {
            PacMode::AutoRules => "按当前路由规则自动生成",
            PacMode::DirectList => "选中 GeoSite 与域名直连，其余代理",
            PacMode::ProxyList => "仅代理选中 GeoSite 与域名，其余直连",
            PacMode::All => "全部代理（本地/局域网直连）",
            PacMode::Custom => "自定义 PAC 脚本",
        }
    }

    pub(super) fn active_pac_strategy(&self) -> Option<(PacMode, Option<String>)> {
        let g = self.status.as_ref().and_then(|s| s.generation.as_ref())?;
        if g.proxy_mode != ProxyMode::Pac {
            return None;
        }
        let pac = g.pac.as_ref()?;
        if let Some(mode) = pac.mode {
            return Some((mode, pac.summary.clone()));
        }
        if let Some(last) = self
            .ui
            .workspace
            .last_pac
            .as_ref()
            .filter(|d| hash(d.script.as_bytes()) == pac.revision)
        {
            return Some((last.mode, None));
        }
        let draft = self.pac_draft_for(&g.candidate.profile_id);
        if hash(draft.script.as_bytes()) == pac.revision {
            return Some((draft.mode, None));
        }
        None
    }
}
