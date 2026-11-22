use super::*;

fn dialog_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).strong().size(18.0));
    if !subtitle.is_empty() {
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(subtitle).size(12.5).weak()).wrap());
    }
    ui.add_space(12.0);
}

fn dialog_actions(ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}

fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    let style = ctx.style();
    egui::Frame::new()
        .fill(style.visuals.window_fill())
        .stroke(egui::Stroke::new(
            1.0,
            crate::widgets::border_strong(style.visuals.dark_mode),
        ))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(20, 16))
        .shadow(style.visuals.popup_shadow)
}

fn show_modal(
    ctx: &egui::Context,
    id: &str,
    max_width: f32,
    body: impl FnOnce(&mut egui::Ui) -> bool,
) -> egui::ModalResponse<bool> {
    egui::Modal::new(egui::Id::new(id))
        .frame(dialog_frame(ctx))
        .backdrop_color(Color32::from_black_alpha(115))
        .show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 88.0).clamp(300.0, max_width));
            body(ui)
        })
}

fn dialog_max_width(dialog: &Dialog) -> f32 {
    match dialog {
        Dialog::Confirm(_) | Dialog::ExitChoice => 500.0,
        Dialog::SshCredential | Dialog::SshHost | Dialog::UnsavedExit(_) => 580.0,
        Dialog::PortForward => 620.0,
        Dialog::HistoryDiff => 700.0,
        Dialog::Create | Dialog::PacBatch => 780.0,
    }
}

fn danger_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(egui::Button::new(
        RichText::new(label).color(ui.visuals().error_fg_color),
    ))
    .clicked()
}

impl Action {
    pub(super) fn copy(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            Action::Switch(_) => (
                "切换编辑对象？",
                "当前有未解决的冲突或损坏缓冲，切换后暂保留本地编辑，不会影响当前运行版本。",
                "保留本地编辑并切换",
            ),
            Action::Discard(_) => (
                "放弃未落盘的本地修改？",
                "恢复到上次已保存的磁盘版本，当前运行版本保持不变。",
                "放弃未保存修改",
            ),
            Action::Delete { .. } => (
                "删除此配置？",
                "将删除配置和本地草稿。上次已应用配置仍被引用时不能删除，请先应用另一份配置。",
                "删除配置",
            ),
            Action::Stop => (
                "停止 Core？",
                "停止当前 Core，并条件恢复本程序接管前的代理设置。其他程序的代理可能仍有效。",
                "停止 Core",
            ),
            Action::Restart => (
                "启动 / 重启上次应用版本？",
                "继续使用已应用配置与冻结的 Core。未应用草稿和下次 Core 选择不会自动生效。",
                "启动 / 重启",
            ),
            Action::Proxy(ProxyMode::Global) => (
                "接管系统代理？",
                "将系统代理指向当前运行配置的本机 HTTP 入口。后台会验证入口，真实网络与实际出口仍需另行验证。",
                "确认接管",
            ),
            Action::Proxy(ProxyMode::Direct) => (
                "恢复接管前的代理设置？",
                "释放本程序持有的代理控制权并恢复先前设置，其他程序的代理可能仍然启用。",
                "恢复先前设置",
            ),
            Action::ExportJson => (
                "导出配置？",
                "配置可能包含节点地址、UUID、密码、私钥等敏感信息；导出文件不受本程序数据目录保护，请妥善保管。",
                "导出…",
            ),
            Action::Proxy(ProxyMode::Pac) => (
                "使用 PAC 自动代理？",
                "后台会发布不可变 PAC 版本并验证后设置当前用户 PAC URL；实际客户端缓存与出口仍需另行验证。",
                "确认切换",
            ),
            Action::AdoptDraft(_) => (
                "用本地内容覆盖保存？",
                "保留当前编辑器中的内容，基于最新磁盘版本直接覆盖保存，不会立即重启 Core。",
                "保留本地内容",
            ),
            Action::ClearPacDomains(_) => (
                "清空 PAC 代理域名清单？",
                "将移除当前配置 PAC 模板中的全部域名，并立即更新草稿；运行版本需在步骤 5 应用后才会变化。",
                "确认清空",
            ),
            Action::ResolveProxyConflict => (
                "保留当前系统代理并解除冲突？",
                "本程序不会修改或覆盖当前 Windows 系统代理设置，仅放弃恢复先前保存的旧根快照并重新以当前系统代理作为基准，从而恢复正常操作与自启能力。",
                "保留当前设置并解除",
            ),
            Action::ResolveStartupConflict => (
                "重新接管登录自启计划任务？",
                "将按当前已保存的启动设置重建或清理本实例对应的 Windows 登录计划任务，并同步更新本地所有权记录；仅影响当前目录对应的 ProxyPilot 计划任务。",
                "确认重新接管",
            ),
        }
    }
}

impl App {
    pub(in crate::app) fn dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.ui.dialog.take() else {
            return;
        };
        let response =
            show_modal(
                ctx,
                "workspace-dialog",
                dialog_max_width(&dialog),
                |ui| match &dialog {
                    Dialog::Create => self.create_dialog_contents(ui),
                    Dialog::Confirm(action) => self.confirm_dialog_contents(ui, action, ctx),
                    Dialog::HistoryDiff => self.history_diff_dialog_contents(ui),
                    Dialog::ExitChoice => self.exit_choice_dialog_contents(ui),
                    Dialog::UnsavedExit(kind) => self.unsaved_exit_dialog_contents(ui, *kind),
                    Dialog::PacBatch => self.pac_batch_dialog_contents(ui),
                    Dialog::PortForward => self.port_forward_dialog_contents(ui),
                    Dialog::SshCredential => self.ssh_credential_dialog_contents(ui),
                    Dialog::SshHost => self.ssh_host_dialog_contents(ui),
                },
            );
        if response.inner && !response.should_close() {
            self.ui.dialog.get_or_insert(dialog);
        }
    }
    fn unsaved_exit_dialog_contents(&mut self, ui: &mut egui::Ui, kind: ExitKind) -> bool {
        dialog_title(
            ui,
            "草稿未保存，暂未退出",
            "部分修改尚未写入磁盘；可返回编辑、重试保存，或明确放弃后退出。",
        );
        egui::ScrollArea::vertical()
            .id_salt("unsaved-exit-body")
            .max_height((ui.ctx().content_rect().height() - 260.0).max(100.0))
            .show(ui, |ui| {
                ui.label("导出可在其他可写目录保留副本；放弃后不会自动恢复这些修改。");
                ui.add_space(6.0);
                let mut profiles: Vec<_> = self.ui.pending_draft_writes.iter().collect();
                profiles.sort();
                for profile in profiles {
                    line(ui, "未保存配置", self.ui.workspace.name(profile));
                }
            });
        dialog_actions(ui);
        let mut keep_open = true;
        ui.horizontal_wrapped(|ui| {
            if ui.button("返回编辑").clicked() {
                keep_open = false;
            }
            if primary(ui, "重试保存并退出", true) {
                self.request_exit(kind);
                if matches!(kind, ExitKind::Shutdown) {
                    self.advance_shutdown();
                }
                keep_open = false;
            }
            #[cfg(windows)]
            if ui.button("导出未保存草稿…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .set_file_name("proxy-pilot-unsaved-drafts.json")
                    .save_file()
            {
                self.export_unsaved_drafts(&path);
            }
            #[cfg(not(windows))]
            if ui.button("复制未保存草稿").clicked() {
                ui.ctx()
                    .copy_text(serde_json::to_string_pretty(&self.unsaved_drafts_json()).unwrap());
            }
            if danger_button(ui, "放弃未保存修改并退出") {
                self.discard_unsaved_and_exit(kind);
                keep_open = false;
            }
        });
        keep_open
    }
    fn exit_choice_dialog_contents(&mut self, ui: &mut egui::Ui) -> bool {
        dialog_title(
            ui,
            "退出 Proxy Pilot",
            "请选择退出方式；默认值会在下次打开此对话框时预选。",
        );
        ui.radio_value(
            &mut self.ui.exit_choice,
            ExitKind::Gui,
            "仅退出界面，后台和 Core 继续运行",
        );
        ui.label(
            RichText::new("托盘仍可重新打开界面；适合暂时收起窗口。")
                .small()
                .weak(),
        );
        ui.add_space(6.0);
        ui.radio_value(
            &mut self.ui.exit_choice,
            ExitKind::Shutdown,
            "退出整个程序，停止 Core 并恢复代理",
        );
        ui.label(
            RichText::new("会等待当前操作完成；结果不明时先恢复连接并查询原操作。")
                .small()
                .weak(),
        );
        ui.add_space(8.0);
        ui.checkbox(&mut self.ui.exit_remember, "记住为默认退出方式");
        dialog_actions(ui);
        ui.horizontal_wrapped(|ui| {
            if ui.button("取消").clicked() {
                return false;
            }
            if primary(ui, "继续退出", !self.exit) {
                let choice = self.ui.exit_choice;
                if self.ui.exit_remember {
                    self.ui.workspace.exit_default = match choice {
                        ExitKind::Gui => crate::workspace::ExitDefault::Gui,
                        ExitKind::Shutdown => crate::workspace::ExitDefault::Shutdown,
                    };
                    self.save_workspace();
                }
                self.request_exit(choice);
                return false;
            }
            true
        })
        .inner
    }
    fn history_diff_dialog_contents(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(preview) = self.ui.history_preview.clone() else {
            return false;
        };
        dialog_title(
            ui,
            "比较历史版本与当前草稿",
            &format!(
                "历史 Core：{}；差异中 - 为当前草稿，+ 为历史版本。载入只替换草稿，不会立即回滚运行版本。",
                preview.core
            ),
        );
        let lines = diff_lines(&self.editor, &preview.json);
        let added = lines.iter().filter(|line| line.starts_with('+')).count();
        let removed = lines.iter().filter(|line| line.starts_with('-')).count();
        if lines.is_empty() {
            ui.label("当前草稿与历史版本内容相同。");
        } else {
            ui.label(format!("共 {added} 行新增、{removed} 行删除。"));
            egui::ScrollArea::vertical()
                .id_salt("history-diff")
                .max_height((ui.ctx().content_rect().height() - 260.0).max(120.0))
                .show(ui, |ui| {
                    for line in lines.iter().take(300) {
                        ui.label(
                            RichText::new(line)
                                .monospace()
                                .color(if line.starts_with('+') {
                                    accent(ui)
                                } else {
                                    ui.visuals().warn_fg_color
                                }),
                        );
                    }
                    if lines.len() > 300 {
                        ui.label(RichText::new("差异较长，仅显示前 300 行。").small().weak());
                    }
                });
        }
        dialog_actions(ui);
        ui.horizontal_wrapped(|ui| {
            if ui.button("保留当前草稿").clicked() {
                self.ui.history_preview = None;
                return false;
            }
            if primary(ui, "载入历史为草稿", true) {
                self.load_history_preview();
                return false;
            }
            true
        })
        .inner
    }
    fn confirm_dialog_contents(
        &mut self,
        ui: &mut egui::Ui,
        action: &Action,
        ctx: &egui::Context,
    ) -> bool {
        let (title, note, confirm) = action.copy();
        dialog_title(ui, title, note);
        if matches!(action, Action::Switch(_) | Action::Proxy(ProxyMode::Global)) {
            egui::ScrollArea::vertical()
                .id_salt(("confirm-body", title))
                .max_height((ctx.content_rect().height() - 220.0).max(100.0))
                .show(ui, |ui| {
                    if let Action::Switch(id) = action {
                        line(ui, "切换到", self.ui.workspace.name(id));
                    }
                    if let Action::Proxy(ProxyMode::Global) = action {
                        line(ui, "使用 HTTP 入站", &self.runtime_http_tag());
                    }
                });
        }
        dialog_actions(ui);
        ui.horizontal_wrapped(|ui| {
            let cancel = ui.button("取消").clicked();
            let confirm = primary(ui, confirm, !self.exit);
            if confirm {
                self.execute_action(action.clone(), ctx);
            }
            !cancel && !confirm
        })
        .inner
    }
    fn create_dialog_contents(&mut self, ui: &mut egui::Ui) -> bool {
        dialog_title(
            ui,
            if self.ui.import_json {
                "添加 / 导入配置"
            } else {
                "新建配置"
            },
            if self.ui.import_json {
                "导入原生 JSON；名称、资源目录和节点参数由你确认，不自动推测。"
            } else {
                "选择场景与连接模板；保存不会启动 Core。"
            },
        );
        let mut can_import = false;
        let mut needs_work = false;
        // Keep the title and action row outside the scrolling body. Long JSON,
        // errors and field advice must never push the confirmation buttons away.
        let body_height = (ui.ctx().content_rect().height() - 210.0).max(160.0);
        egui::ScrollArea::vertical()
            .id_salt("create-body")
            .max_height(body_height)
            .show(ui, |ui| {
                let body_top = ui.cursor().top();
                ui.set_min_width(ui.available_width());
                ui.label("配置显示名称");
                ui.add(singleline(&mut self.ui.new_name, f32::INFINITY).hint_text("例如：工作网络"));
                ui.label(RichText::new("名称支持中文；原有字段与路由顺序保留，不自动猜测节点参数。").small().weak());
                if self.ui.import_json {
                    #[cfg(windows)]
                    if ui.button("从 JSON 文件读取…").clicked()
                        && let Some(path) = rfd::FileDialog::new().add_filter("原生 Xray JSON", &["json"]).pick_file()
                    {
                        match std::fs::read_to_string(&path) {
                            Ok(json) => {
                                self.ui.new_json = json;
                                if self.ui.new_name.trim().is_empty() {
                                    self.ui.new_name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                                }
                                self.ui.import_source = Some(path);
                                self.ui.import_error = None;
                            }
                            Err(error) => self.ui.import_error = Some(error.to_string()),
                        }
                    }
                    if let Some(path) = &self.ui.import_source {
                        ui.add(egui::Label::new(RichText::new(display_path(path)).small().weak()).wrap());
                        ui.checkbox(&mut self.ui.import_use_source_assets, "使用配置文件所在目录查找资源")
                            .on_hover_text("默认沿用设置中的资源目录。仅在 geo 文件或相对证书路径基于配置所在目录时勾选。");
                    }
                    if let Some(error) = &self.ui.import_error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    ui.horizontal(|ui| {
                        ui.label("原生 JSON 内容");
                        if ui.button("格式化 JSON").clicked()
                            && let Ok(value) = proxy_config::parse_document(self.ui.new_json.as_bytes())
                        {
                            self.ui.new_json = serde_json::to_string_pretty(&value).unwrap();
                        }
                    });
                    let used_height = ui.cursor().top() - body_top;
                    let height = (body_height - used_height - 100.0).max(100.0);
                    json_editor(ui, &mut self.ui.new_json, "import-json", height);
                } else {
                    use crate::guided_config::{Scene, Template};
                    let desired_key = (
                        self.ui.new_scene,
                        self.ui.new_template,
                        self.ui.new_split_preset,
                        self.ui.new_split_sniffing,
                    );
                    if self.ui.new_config_key != Some(desired_key) {
                        self.ui.new_config_key = Some(desired_key);
                        if !self.ui.new_share_imported {
                            self.ui.new_json = serde_json::to_string_pretty(
                                &crate::guided_config::scene_with_options(
                                    self.ui.new_scene,
                                    self.ui.new_template,
                                    self.ui.new_split_preset,
                                    self.ui.new_split_sniffing,
                                ),
                            )
                            .unwrap();
                            self.ui.new_node_names.clear();
                            if self.ui.new_scene != Scene::Direct {
                                self.ui.new_node_names.insert(
                                    "node-1".into(),
                                    format!("{} 节点", self.ui.new_template.label()),
                                );
                            }
                            self.ui.new_editor_epoch = self.ui.new_editor_epoch.wrapping_add(1);
                        } else if let Ok(mut value) =
                            proxy_config::parse_document(self.ui.new_json.as_bytes())
                        {
                            value["routing"] = serde_json::json!({"rules":[]});
                            if self.ui.new_scene == Scene::Split {
                                let mut st = crate::guided_config::State::default();
                                if self.ui.new_split_preset {
                                    crate::guided_config::apply_geosite_bypass_preset(&mut value, &mut st);
                                }
                                if self.ui.new_split_sniffing {
                                    crate::guided_config::enable_inbound_sniffing(&mut value, &mut st);
                                }
                            }
                            self.ui.new_json = serde_json::to_string_pretty(&value).unwrap();
                        }
                    }

                    ui.label("选择使用场景");
                    ui.horizontal_wrapped(|ui| {
                        for scene in Scene::ALL {
                            ui.selectable_value(&mut self.ui.new_scene, scene, scene.label());
                        }
                    });
                    if self.ui.new_scene == Scene::Split {
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut self.ui.new_split_preset, "添加国内外分流预设 (GeoSite / GeoIP 直连)");
                            ui.checkbox(&mut self.ui.new_split_sniffing, "开启本地入口 Sniffing");
                        });
                        if self.ui.new_split_preset {
                            let asset_dir = resolve_path(&self.root, std::path::Path::new(self.asset_root.trim()));
                            if !asset_dir.join("geosite.dat").is_file() || !asset_dir.join("geoip.dat").is_file() {
                                ui.label(
                                    RichText::new("提示：当前资源目录暂未检测到 geosite.dat / geoip.dat，可在「设置 → Core 与依赖」一键下载。")
                                        .small()
                                        .color(ui.visuals().warn_fg_color),
                                );
                            }
                        }
                    }
                    if self.ui.new_scene != Scene::Direct {
                        ui.separator();
                        ui.label("从分享链接快速填入（纯本地离线解析，支持 vless / trojan / ss / vmess / socks / http，可多行）");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.ui.new_share_input)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .hint_text("粘贴 vless://... 或多行分享链接"),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    !self.ui.new_share_input.trim().is_empty(),
                                    egui::Button::new("解析并填入"),
                                )
                                .clicked()
                            {
                                let batch = proxy_config::share_link::parse_share_links(&self.ui.new_share_input);
                                if batch.nodes.is_empty() {
                                    self.ui.new_share_error = Some(
                                        batch
                                            .errors
                                            .first()
                                            .map(|e| format!("第 {} 行：{}", e.line, e.message))
                                            .unwrap_or_else(|| "未识别到有效的分享链接".into()),
                                    );
                                } else {
                                    let mut base = crate::guided_config::scene_with_options(
                                        Scene::Direct,
                                        self.ui.new_template,
                                        false,
                                        false,
                                    );
                                    if self.ui.new_scene == Scene::Split {
                                        let mut st = crate::guided_config::State::default();
                                        if self.ui.new_split_preset {
                                            crate::guided_config::apply_geosite_bypass_preset(&mut base, &mut st);
                                        }
                                        if self.ui.new_split_sniffing {
                                            crate::guided_config::enable_inbound_sniffing(&mut base, &mut st);
                                        }
                                    }
                                    let mut names = std::collections::BTreeMap::new();
                                    let first_name = batch.nodes.first().and_then(|n| n.name.clone());
                                    match crate::guided_config::append_share_nodes(
                                        &mut base,
                                        &mut names,
                                        batch.nodes,
                                        Some(0),
                                    ) {
                                        Ok((count, warnings)) => {
                                            self.ui.new_json = serde_json::to_string_pretty(&base).unwrap();
                                            self.ui.new_node_names = names;
                                            if self.ui.new_name.trim().is_empty()
                                                && let Some(name) = first_name
                                            {
                                                self.ui.new_name = name.chars().take(80).collect();
                                            }
                                            self.ui.new_share_imported = true;
                                            self.ui.new_share_warnings = warnings;
                                            self.ui.new_share_error = if batch.errors.is_empty() {
                                                None
                                            } else {
                                                Some(format!(
                                                    "已导入 {count} 个节点；另有 {} 行未能解析（如第 {} 行：{}）。",
                                                    batch.errors.len(),
                                                    batch.errors[0].line,
                                                    batch.errors[0].message
                                                ))
                                            };
                                            self.ui.new_editor_epoch = self.ui.new_editor_epoch.wrapping_add(1);
                                        }
                                        Err(err) => self.ui.new_share_error = Some(err.into()),
                                    }
                                }
                            }
                            if self.ui.new_share_imported && ui.button("恢复模板手动填写").clicked() {
                                self.ui.new_share_imported = false;
                                self.ui.new_config_key = None;
                                self.ui.new_share_warnings.clear();
                                self.ui.new_share_error = None;
                            }
                        });
                        if let Some(err) = &self.ui.new_share_error {
                            ui.colored_label(ui.visuals().warn_fg_color, err);
                        }
                        for warn in &self.ui.new_share_warnings {
                            ui.colored_label(ui.visuals().warn_fg_color, warn);
                        }
                        if !self.ui.new_share_imported {
                            ui.label("连接模板");
                            egui::ComboBox::from_id_salt("create-template")
                                .selected_text(self.ui.new_template.label())
                                .show_ui(ui, |ui| {
                                    for template in Template::ALL {
                                        ui.selectable_value(
                                            &mut self.ui.new_template,
                                            template,
                                            template.label(),
                                        );
                                    }
                                });
                        }
                    }

                    if let Ok(mut value) = proxy_config::parse_document(self.ui.new_json.as_bytes()) {
                        let proxy_count = value
                            .get("outbounds")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter(|n| {
                                        !matches!(
                                            n.get("protocol").and_then(|p| p.as_str()),
                                            Some("freedom" | "blackhole")
                                        )
                                    })
                                    .count()
                            })
                            .unwrap_or(0);
                        let mut changed = false;
                        if self.ui.new_scene != Scene::Direct && proxy_count == 1 {
                            let tag = value["outbounds"][0]["tag"]
                                .as_str()
                                .unwrap_or("node-1")
                                .to_owned();
                            ui.label("节点显示名称");
                            let name = self.ui.new_node_names.entry(tag).or_default();
                            ui.add(singleline(name, f32::INFINITY).char_limit(80));
                            changed |= ui
                                .push_id(("new-node", self.ui.new_editor_epoch), |ui| {
                                    crate::guided_config::node_form(ui, &mut value["outbounds"][0], None)
                                })
                                .inner;
                        } else if proxy_count > 1 {
                            ui.label(RichText::new(format!("已从分享链接解析 {proxy_count} 个节点（首个为默认访问节点）：")).strong());
                            if let Some(outbounds) = value.get("outbounds").and_then(|v| v.as_array()) {
                                for node in outbounds.iter().take(proxy_count) {
                                    let label = crate::guided_config::node_name(node, &self.ui.new_node_names);
                                    let proto = node["protocol"].as_str().unwrap_or("");
                                    let addr = node
                                        .pointer("/settings/vnext/0/address")
                                        .or_else(|| node.pointer("/settings/servers/0/address"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    ui.label(format!("• {label} ({proto}) · {addr}"));
                                }
                            }
                        }
                        if changed {
                            self.ui.new_json = serde_json::to_string_pretty(&value).unwrap();
                        }
                        crate::guided_config::summary(ui, &value, &self.ui.new_node_names);
                    }
                    ui.label(
                        RichText::new("模板自动准备本机 HTTP / SOCKS 入口；保存后仍可随时在「节点」与「分流规则」调整。")
                            .small()
                            .weak(),
                    );
                }
                match proxy_config::parse_document(self.ui.new_json.as_bytes()) {
                    Ok(value) => {
                        can_import = true;
                        needs_work = config_review(ui, &value);
                    }
                    Err(error) => {
                        ui.colored_label(ui.visuals().error_fg_color, "JSON 暂时无法解析，请修正后保存；当前输入会保留。");
                        ui.add(egui::Label::new(error.message).wrap());
                    }
                }
            });
        dialog_actions(ui);
        ui.horizontal_wrapped(|ui| {
            if ui.button("取消").clicked() {
                return false;
            }
            if primary(
                ui,
                if needs_work {
                    "保存待补充配置"
                } else {
                    "添加配置"
                },
                can_import && self.write_blocker().is_none() && !self.ui.new_name.trim().is_empty(),
            ) {
                if self.ui.import_use_source_assets
                    && let Some(parent) = self.ui.import_source.as_ref().and_then(|p| p.parent())
                {
                    self.asset_root = to_portable_string(&self.root, &parent.to_string_lossy());
                    self.settings_edited();
                }
                let profile_id = format!("profile-{}", id());
                self.ui.workspace.names.insert(
                    profile_id.clone(),
                    self.ui.new_name.trim().chars().take(80).collect(),
                );
                if !self.ui.new_node_names.is_empty() {
                    self.ui
                        .workspace
                        .node_names
                        .insert(profile_id.clone(), self.ui.new_node_names.clone());
                }
                self.save_workspace();
                self.send(Command::CreateProfile {
                    profile_id,
                    json: self.ui.new_json.clone(),
                });
                return false;
            }
            ui.label(RichText::new("仅保存配置，不启动 Core").small().weak());
            true
        })
        .inner
    }
    pub(in crate::app) fn execute_action(&mut self, action: Action, _ctx: &egui::Context) {
        match action {
            Action::Switch(id) => self.switch_profile(id),
            Action::Discard(profile) => {
                if self.selected_profile != profile || self.profile_id != profile {
                    self.message = "编辑对象已变化，请重新确认放弃草稿。".into();
                    return;
                }
                self.ui.unreadable_drafts.remove(&profile);
                self.editor = self.saved_json.clone();
                self.ui.number_inputs.clear();
                self.ui.draft_conflict = false;
                self.draft_edited();
                self.stash_draft();
            }
            Action::AdoptDraft(profile) => {
                if self.selected_profile != profile || self.profile_id != profile {
                    self.message = "编辑对象已变化，请重新确认草稿处理。".into();
                    return;
                }
                self.ui.draft_conflict = false;
                self.draft_edited();
                self.stash_draft();
            }
            Action::ClearPacDomains(profile) => {
                self.clear_all_pac_domains_for(&profile);
            }
            Action::Delete { profile, revision } => self.send(Command::DeleteProfile {
                profile_id: profile,
                expected_revision: revision,
            }),
            Action::ExportJson => {
                #[cfg(windows)]
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("config.json")
                    .save_file()
                {
                    if let Err(error) = std::fs::write(path, &self.editor) {
                        self.fail(error.into(), String::new());
                    } else {
                        self.message = "配置已导出；文件可能包含认证信息。".into();
                    }
                }
                #[cfg(not(windows))]
                {
                    self.message = "当前平台不提供图形文件选择器。".into();
                }
            }
            Action::Stop => self.send(Command::StopCore),
            Action::Restart => self.send(Command::RestartCore),
            Action::ResolveProxyConflict => self.send(Command::ResolveProxyConflict),
            Action::ResolveStartupConflict => self.send(Command::ResolveStartupConflict),
            Action::Proxy(mode) => {
                let http_tag = self.runtime_http_tag();
                let pac = if mode == ProxyMode::Pac {
                    let (active_profile, active_json) = self.active_pac_target();
                    Some(self.compiled_pac_draft_for(
                        &active_profile,
                        &active_json,
                        Some(&http_tag),
                    ))
                } else {
                    None
                };
                self.send(Command::SetProxyMode {
                    mode,
                    http_inbound_tag: http_tag,
                    pac,
                });
            }
        }
    }
    pub(in crate::app) fn runtime_http_tag(&self) -> String {
        if let Some(g) = self.status.as_ref().and_then(|s| s.generation.as_ref()) {
            if let Some(config) = self.ui.runtime.as_ref().filter(|c| c.generation_id == g.id)
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&config.json)
                && let Some(inbounds) = value.get("inbounds").and_then(|v| v.as_array())
            {
                let tags: Vec<_> = inbounds
                    .iter()
                    .filter(|i| i.get("protocol").and_then(|v| v.as_str()) == Some("http"))
                    .filter_map(|i| i.get("tag").and_then(|v| v.as_str()))
                    .collect();
                if tags.contains(&g.http_inbound_tag.as_str()) {
                    return g.http_inbound_tag.clone();
                }
                if let Some(tag) = tags.first() {
                    return (*tag).into();
                }
            }
            return g.http_inbound_tag.clone();
        }
        String::new()
    }
    pub(in crate::app) fn preview(&mut self, ctx: &egui::Context) {
        if !self.candidate_matches() {
            self.ui.preview_open = false;
            return;
        }
        let candidate = self.candidate.clone().unwrap();
        let response = show_modal(ctx, "candidate-preview", 780.0, |ui| {
            dialog_title(
                ui,
                "预览本次应用",
                "以下内容将作为同一个运行版本应用；确认后才会切换 Core 与配置。",
            );
            let body_height = (ctx.content_rect().height() - 260.0).max(120.0);
            egui::ScrollArea::vertical()
                .id_salt(("candidate-preview-body", &candidate.id))
                .max_height(body_height)
                .show(ui, |ui| {
                    ui.push_id(&candidate.id, |ui| {
                        self.preview_body(ui, &candidate, body_height)
                    });
                });
            dialog_actions(ui);
            ui.horizontal_wrapped(|ui| {
                let cancel = ui.button("返回编辑").clicked();
                let apply = primary(
                    ui,
                    if self.is_running() {
                        "确认应用"
                    } else {
                        "确认并启动"
                    },
                    self.write_blocker().is_none() && self.candidate_matches(),
                );
                if apply {
                    let pac = if self.mode == ProxyMode::Pac {
                        let editor = self.editor.clone();
                        let preferred = self.http_tag.clone();
                        Some(self.compiled_pac_draft_for(
                            &candidate.profile_id,
                            &editor,
                            Some(&preferred),
                        ))
                    } else {
                        None
                    };
                    self.send(Command::ApplyProfile {
                        candidate_id: candidate.id,
                        candidate_digest: candidate.digest,
                        proxy_mode: self.mode,
                        http_inbound_tag: self.http_tag.clone(),
                        pac,
                    });
                }
                cancel || apply
            })
            .inner
        });
        if response.inner || response.should_close() {
            self.ui.preview_open = false;
        }
    }
    fn preview_body(&self, ui: &mut egui::Ui, candidate: &Candidate, viewport_height: f32) {
        let body_top = ui.cursor().top();
        line(
            ui,
            "目标配置",
            self.ui.workspace.name(&candidate.profile_id),
        );
        line(ui, "通过校验的 Core", &candidate.core.version);
        line(ui, "系统代理方式", mode_name(self.mode));
        if matches!(self.mode, ProxyMode::Global | ProxyMode::Pac) {
            line(ui, "接管使用的 HTTP Tag", &self.http_tag);
        }
        if self.mode == ProxyMode::Pac {
            let draft = self.pac_draft();
            line(ui, "PAC 服务端口", &draft.port.to_string());
            line(
                ui,
                "PAC 策略",
                match draft.mode {
                    proxy_model::PacMode::AutoRules => "按路由规则自动生成（支持 GeoSite）",
                    proxy_model::PacMode::DirectList => "选中 GeoSite 与域名直连，其余代理",
                    proxy_model::PacMode::ProxyList => "仅代理选中 GeoSite 与域名，其余直连",
                    proxy_model::PacMode::All => "全部代理（本地/局域网直连）",
                    proxy_model::PacMode::Custom => "自定义 PAC 脚本",
                },
            );
        }
        if let Ok(value) = proxy_config::parse_document(self.editor.as_bytes()) {
            if Self::has_exposed_port_forward(&value) {
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "风险提示：包含监听于 0.0.0.0 或非回环地址的 dokodemo-door 端口转发（无密码认证保护），局域网设备可直接连接。",
                        )
                        .color(ui.visuals().warn_fg_color),
                    )
                    .wrap(),
                );
            }
            let names = self
                .ui
                .workspace
                .node_names
                .get(&candidate.profile_id)
                .cloned()
                .unwrap_or_default();
            crate::guided_config::summary(ui, &value, &names);
        }
        if !candidate.resources.complete {
            ui.add(egui::Label::new(
                RichText::new("配置已通过 Core 校验。未分类字段无法冻结；启动和恢复时仍需 Core 接受，失败会停止并报告。")
                    .color(ui.visuals().warn_fg_color),
            ).wrap());
            if !candidate.resources.dynamic.is_empty() {
                ui.label(format!(
                    "其中 {} 项为显式动态外部文件；自动连接、异常重试及自动恢复旧 Core 不可用。",
                    candidate.resources.dynamic.len()
                ));
            }
        }
        ui.separator();
        // The viewport bounds scrolling, while its document stays unbounded.
        // Never constrain the document Ui to the viewport height: expanded
        // details and long diagnostics must retain their natural row heights.
        let used_height = ui.cursor().top() - body_top;
        let json_height = (viewport_height - used_height - 200.0).max(100.0);
        ui.columns(2, |columns| {
            columns[0].label(if self.is_running() {
                "当前运行版本"
            } else {
                "上次应用版本（当前未运行）"
            });
            let mut old = self
                .ui
                .runtime
                .as_ref()
                .filter(|r| {
                    self.status
                        .as_ref()
                        .and_then(|s| s.generation.as_ref())
                        .is_some_and(|g| g.id == r.generation_id)
                })
                .map(|r| r.json.clone())
                .unwrap_or_else(|| "暂无已读取的运行配置".into());
            json_viewer(
                &mut columns[0],
                &mut old,
                "preview-running-json",
                json_height,
            );
            columns[1].label("待应用草稿");
            let mut json = self.editor.clone();
            json_viewer(
                &mut columns[1],
                &mut json,
                "preview-draft-json",
                json_height,
            );
        });
        egui::CollapsingHeader::new("与此配置已保存版本的差异（最多展示 200 行）").show(ui, |ui| {
            let lines = diff_lines(&self.saved_json, &self.editor);
            if lines.is_empty() {
                ui.label("与此配置的已保存版本相同。");
            }
            for line in lines.into_iter().take(200) {
                ui.label(
                    RichText::new(&line)
                        .monospace()
                        .color(if line.starts_with('+') {
                            accent(ui)
                        } else {
                            ui.visuals().warn_fg_color
                        }),
                );
            }
        });
        ui.separator();
        line(
            ui,
            "冻结资源文件数",
            &candidate.resources.files.len().to_string(),
        );
        if !candidate.resources.notes.is_empty() {
            egui::CollapsingHeader::new(format!(
                "查看 {} 项资源与自动恢复限制",
                candidate.resources.notes.len()
            ))
            .id_salt("resource-coverage-notes")
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "以下是管理器的自动恢复覆盖范围提示，不表示配置非法；原始字段会保留。",
                        )
                        .small()
                        .weak(),
                    )
                    .wrap(),
                );
                egui::ScrollArea::vertical()
                    .id_salt("resource-coverage-notes-scroll")
                    .max_height((viewport_height * 0.3).clamp(100.0, 220.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for note in &candidate.resources.notes {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(resource_note(note))
                                        .color(ui.visuals().warn_fg_color),
                                )
                                .wrap(),
                            );
                        }
                    });
            });
        }
        egui::CollapsingHeader::new("冻结资源清单与 Core 身份").show(ui, |ui| {
            ui.monospace(format!("Core SHA-256: {}", candidate.core.sha256));
            for file in &candidate.resources.files {
                ui.monospace(format!(
                    "{}\nSHA-256: {}",
                    display_path(&file.relative_path),
                    file.sha256
                ));
            }
        });
        ui.label(RichText::new("已由真实 Core 执行配置校验；这不代表远端节点可连接。正式应用时仍需检查依赖和入口。").small().weak());
    }
    fn pac_batch_dialog_contents(&mut self, ui: &mut egui::Ui) -> bool {
        dialog_title(
            ui,
            "批量导入 / 编辑 PAC 域名",
            "每行一个域名，或用逗号、分号、空格分隔；支持 example.com、*.example.com、full:example.com，自动忽略空行与 # / // 注释。",
        );
        let body_height = (ui.ctx().content_rect().height() - 260.0).max(160.0);
        egui::ScrollArea::vertical()
            .id_salt("pac-batch-body")
            .max_height(body_height)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(
                        &mut self.ui.pac_batch_replace,
                        false,
                        "追加到现有清单（自动去重）",
                    );
                    ui.radio_value(&mut self.ui.pac_batch_replace, true, "替换当前全部清单");
                    if ui.button("填入当前清单").clicked() {
                        let (target_profile, _) = self.active_pac_target();
                        self.ui.pac_batch_input = proxy_pac::format_domain_list(
                            &self.pac_draft_for(&target_profile).domains,
                        );
                        self.ui.pac_batch_report = None;
                    }
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt("pac-batch-text")
                    .max_height((body_height - 90.0).clamp(120.0, 320.0))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.ui.pac_batch_input)
                                .code_editor()
                                .desired_rows(12)
                                .desired_width(f32::INFINITY)
                                .hint_text("example.com\n*.internal.example\nfull:api.example.org"),
                        );
                    });
                if let Some(report) = &self.ui.pac_batch_report {
                    ui.add_space(6.0);
                    ui.colored_label(ui.visuals().warn_fg_color, report);
                }
            });
        dialog_actions(ui);
        let mut keep_open = true;
        ui.horizontal_wrapped(|ui| {
            if ui.button("取消").clicked() {
                keep_open = false;
            }
            let can_apply = !self.ui.pac_batch_input.trim().is_empty() || self.ui.pac_batch_replace;
            if primary(ui, "导入并保存", can_apply) {
                let batch = self.ui.pac_batch_input.clone();
                let replace = self.ui.pac_batch_replace;
                if self.apply_pac_batch(&batch, replace) {
                    self.ui.pac_batch_input.clear();
                    self.ui.pac_batch_report = None;
                    keep_open = false;
                }
            }
        });
        keep_open
    }
    fn port_forward_dialog_contents(&mut self, ui: &mut egui::Ui) -> bool {
        let is_edit = self.ui.pf_form.editing_tag.is_some()
            || self.ui.pf_form.editing_paused_index.is_some()
            || self.ui.pf_form.editing_ssh_id.is_some();
        let is_clone = self.ui.pf_form.is_clone;
        let mut parsed_json = serde_json::from_str::<serde_json::Value>(&self.editor).ok();

        if parsed_json.is_none()
            && self.ui.pf_form.editing_tag.is_none()
            && self.ui.pf_form.editing_paused_index.is_none()
        {
            self.ui.pf_form.outbound_is_ssh = true;
            self.ui.pf_form.network = "tcp".into();
        }

        let subtitle = if let Some(tag) = &self.ui.pf_form.editing_tag {
            format!("入站 Tag：{tag} · 将自动同步对应 dokodemo-door 入站与置顶 inboundTag 路由规则")
        } else if self.ui.pf_form.outbound_is_ssh {
            "直接由后台通过 SSH 跳板机转发本机 TCP 端口，独立于 Xray JSON 与 Core 运行。".into()
        } else {
            "将自动在配置中添加 dokodemo-door 入站，并在分流规则顶部维护对应的 inboundTag 规则。"
                .into()
        };
        dialog_title(
            ui,
            if is_edit {
                "编辑端口转发"
            } else if is_clone {
                "克隆 SSH 隧道"
            } else {
                "添加端口转发"
            },
            &subtitle,
        );

        if parsed_json.is_none() && !self.ui.pf_form.outbound_is_ssh {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "当前配置 JSON 语法有误，请先在「更多与源码 → 原始 JSON」中修正。",
            );
            dialog_actions(ui);
            return !ui.button("关闭").clicked();
        }

        let node_names = self
            .ui
            .workspace
            .node_names
            .get(&self.profile_id)
            .cloned()
            .unwrap_or_default();
        let outbounds = parsed_json
            .as_ref()
            .map(|v| crate::port_forward::available_outbounds(v, &node_names))
            .unwrap_or_default();
        let ssh_hosts = self.ui.ssh_state.hosts.clone();
        let ssh_forwards = self.ui.ssh_state.forwards.clone();
        let paused_pf = self
            .ui
            .workspace
            .paused_port_forwards
            .get(&self.profile_id)
            .cloned()
            .unwrap_or_default();

        if !self.ui.pf_form.outbound_is_ssh
            && self.ui.pf_form.outbound_tag.is_empty()
            && let Some((first, _)) = outbounds.first()
        {
            self.ui.pf_form.outbound_tag = first.clone();
        }
        if self.ui.pf_form.outbound_is_ssh {
            self.ui.pf_form.network = "tcp".into();
        }

        let body_height = (ui.ctx().content_rect().height() - 210.0).max(180.0);
        let mut open_host_modal: Option<Option<crate::port_forward::ParsedSshJumpHint>> = None;
        egui::ScrollArea::vertical()
            .id_salt("port-forward-dialog-body")
            .max_height(body_height)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if !is_edit && !is_clone {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("通道类型");
                        if crate::widgets::seg_button(
                            ui,
                            "SSH 隧道",
                            self.ui.pf_form.outbound_is_ssh,
                            true,
                        )
                        .clicked()
                        {
                            self.ui.pf_form.outbound_is_ssh = true;
                            self.ui.pf_form.network = "tcp".into();
                            if self.ui.pf_form.ssh_host_id.is_empty()
                                && let Some(first) = ssh_hosts.first()
                            {
                                self.ui.pf_form.ssh_host_id = first.id.clone();
                            }
                            if self.ui.pf_form.target_host.trim().is_empty() {
                                self.ui.pf_form.target_host = "127.0.0.1".into();
                            }
                            self.ui.pf_error = None;
                        }
                        let xray_btn = crate::widgets::seg_button(
                            ui,
                            "Xray 转发",
                            !self.ui.pf_form.outbound_is_ssh,
                            parsed_json.is_some(),
                        );
                        if parsed_json.is_none() {
                            xray_btn.on_hover_text(
                                "当前 Xray 草稿存在 JSON 语法错误，暂不可创建 Xray 转发；可直接使用 SSH 隧道",
                            );
                        } else if xray_btn.clicked() {
                            self.ui.pf_form.outbound_is_ssh = false;
                            if self.ui.pf_form.outbound_tag.is_empty()
                                && let Some((first, _)) = outbounds.first()
                            {
                                self.ui.pf_form.outbound_tag = first.clone();
                            }
                            self.ui.pf_error = None;
                        }
                    });
                    ui.add_space(6.0);
                }

                if self.ui.pf_form.outbound_is_ssh {
                    // Quick ssh -L command parser subcard
                    crate::widgets::subcard(ui, None, true, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("粘贴 ssh -L 命令自动填入（可选）").small());
                            if crate::widgets::compact_button(ui, "填入示例").clicked() {
                                self.ui.pf_form.ssh_cmd_input =
                                    "ssh -L 15432:10.0.1.8:5432 ops@bastion.corp -p 2222".into();
                                apply_parsed_ssh_cmd(&mut self.ui.pf_form, &ssh_hosts);
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            let cmd_w = (ui.available_width() - 96.0).max(160.0);
                            crate::widgets::singleline_select_all(
                                ui,
                                &mut self.ui.pf_form.ssh_cmd_input,
                                cmd_w,
                                "ssh -L 15432:10.0.1.8:5432 ops@bastion.corp -p 2222",
                            );
                            if ui.button("解析填入").clicked() {
                                apply_parsed_ssh_cmd(&mut self.ui.pf_form, &ssh_hosts);
                            }
                        });
                        if let Some((is_err, ref msg)) = self.ui.pf_form.ssh_cmd_feedback {
                            crate::widgets::banner_box(
                                ui,
                                if is_err {
                                    crate::widgets::BadgeTone::Warn
                                } else {
                                    crate::widgets::BadgeTone::Accent
                                },
                                msg,
                            );
                        }
                        if self.ui.pf_form.parsed_ssh_hint.is_some()
                            && ui.button("按解析结果新建跳板机").clicked()
                        {
                            open_host_modal = Some(self.ui.pf_form.parsed_ssh_hint.clone());
                        }
                    });
                    ui.add_space(6.0);

                    // 1. SSH Jump Host
                    ui.label("1. SSH 跳板机");
                    let selected_host_label = ssh_hosts
                        .iter()
                        .find(|h| h.id == self.ui.pf_form.ssh_host_id)
                        .map(|h| format!("{} ({}:{})", h.name, h.host, h.port))
                        .unwrap_or_else(|| {
                            if ssh_hosts.is_empty() {
                                "暂无 SSH 跳板机".into()
                            } else {
                                "请选择 SSH 跳板机".into()
                            }
                        });
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("pf-ssh-host-select")
                            .width((ui.available_width() - 140.0).clamp(200.0, 440.0))
                            .selected_text(selected_host_label)
                            .show_ui(ui, |ui| {
                                for host in &ssh_hosts {
                                    let is_sel = self.ui.pf_form.ssh_host_id == host.id;
                                    let label =
                                        format!("{} ({}:{})", host.name, host.host, host.port);
                                    if ui.selectable_label(is_sel, label).clicked() {
                                        self.ui.pf_form.ssh_host_id = host.id.clone();
                                        self.ui.pf_form.parsed_ssh_hint = None;
                                        self.ui.pf_error = None;
                                    }
                                }
                            });
                        if ui.button("+ 新建跳板机").clicked() {
                            open_host_modal = Some(self.ui.pf_form.parsed_ssh_hint.clone());
                        }
                    });
                    if ssh_hosts.is_empty() {
                        crate::widgets::banner_box(
                            ui,
                            crate::widgets::BadgeTone::Accent,
                            "暂无跳板机，点右侧可在同一弹窗内一并填写凭证并创建，完成后自动回到此处继续。",
                        );
                    }

                    // 2. Target Host & Port with Preset Chips
                    ui.label("2. 目标地址与端口（支持直接粘贴 host:port 自动拆分）");
                    ui.horizontal_wrapped(|ui| {
                        let is_local_target = self.ui.pf_form.target_host.trim() == "127.0.0.1";
                        if crate::widgets::compact_chip(
                            ui,
                            "跳板机本机 (127.0.0.1)",
                            is_local_target,
                        )
                        .clicked()
                        {
                            self.ui.pf_form.target_host = "127.0.0.1".into();
                            self.ui.pf_error = None;
                        }
                        for (port, _, chip_label) in crate::port_forward::COMMON_TARGET_PORTS {
                            let port_str = port.to_string();
                            let active = self.ui.pf_form.target_port.trim() == port_str;
                            if crate::widgets::compact_chip(ui, chip_label, active).clicked() {
                                self.ui.pf_form.target_port = port_str;
                                self.ui.pf_error = None;
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("目标地址");
                        let host_w = (ui.available_width() - 230.0).max(150.0);
                        if crate::widgets::singleline_select_all(
                            ui,
                            &mut self.ui.pf_form.target_host,
                            host_w,
                            "10.0.1.8 或 db.internal:3306",
                        )
                        .changed()
                        {
                            if let Some((h, p)) = crate::port_forward::parse_host_port_paste(
                                &self.ui.pf_form.target_host,
                            ) {
                                self.ui.pf_form.target_host = h;
                                self.ui.pf_form.target_port = p;
                            }
                            self.ui.pf_error = None;
                        }
                        ui.label("目标端口");
                        if crate::widgets::singleline_select_all(
                            ui,
                            &mut self.ui.pf_form.target_port,
                            100.0,
                            "5432",
                        )
                        .changed()
                        {
                            self.ui.pf_error = None;
                        }
                    });

                    // 3. Listen Scope & Local Port + Conflict Quick Fix
                    render_listen_scope_and_port(
                        ui,
                        &mut self.ui.pf_form,
                        &mut self.ui.pf_error,
                        parsed_json.as_ref(),
                        &paused_pf,
                        &ssh_forwards,
                        "SSH 隧道监听非回环地址后，同局域网设备也可直接访问此本地转发端口，请确认目标服务安全策略。",
                    );

                    // 4. Optional Remark Name with Dynamic Auto-Name Hint
                    ui.label("4. 备注名称（可选）");
                    let t_port = self
                        .ui
                        .pf_form
                        .target_port
                        .trim()
                        .parse::<u16>()
                        .unwrap_or(0);
                    let name_hint = format!(
                        "留空自动命名：{}",
                        crate::port_forward::auto_ssh_forward_name(
                            &self.ui.pf_form.target_host,
                            t_port
                        )
                    );
                    crate::widgets::singleline_select_all(
                        ui,
                        &mut self.ui.pf_form.name,
                        ui.available_width(),
                        &name_hint,
                    );
                } else {
                    // Xray Port Forward Mode
                    ui.label("备注名称（可选，仅保存在工作区界面识别）");
                    crate::widgets::singleline_select_all(
                        ui,
                        &mut self.ui.pf_form.name,
                        ui.available_width(),
                        "例如：公司内网 MySQL",
                    );

                    ui.add_space(6.0);
                    render_listen_scope_and_port(
                        ui,
                        &mut self.ui.pf_form,
                        &mut self.ui.pf_error,
                        parsed_json.as_ref(),
                        &paused_pf,
                        &ssh_forwards,
                        "dokodemo-door 无认证保护：绑定非回环地址后，局域网内任意设备可直接连接此端口转发。",
                    );

                    ui.add_space(6.0);
                    ui.label("传输协议");
                    ui.horizontal_wrapped(|ui| {
                        for (net, label) in
                            [("tcp", "TCP"), ("udp", "UDP"), ("tcp,udp", "TCP + UDP")]
                        {
                            if crate::widgets::seg_button(
                                ui,
                                label,
                                self.ui.pf_form.network == net,
                                true,
                            )
                            .clicked()
                            {
                                self.ui.pf_form.network = net.into();
                                self.ui.pf_error = None;
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.label("目标地址与端口（支持直接粘贴 host:port 自动拆分）");
                    ui.horizontal_wrapped(|ui| {
                        ui.label("目标地址");
                        let host_w = (ui.available_width() - 230.0).max(150.0);
                        if crate::widgets::singleline_select_all(
                            ui,
                            &mut self.ui.pf_form.target_host,
                            host_w,
                            "10.0.0.5 或 db.internal:3306",
                        )
                        .changed()
                        {
                            if let Some((h, p)) = crate::port_forward::parse_host_port_paste(
                                &self.ui.pf_form.target_host,
                            ) {
                                self.ui.pf_form.target_host = h;
                                self.ui.pf_form.target_port = p;
                            }
                            self.ui.pf_error = None;
                        }
                        ui.label("目标端口");
                        if crate::widgets::singleline_select_all(
                            ui,
                            &mut self.ui.pf_form.target_port,
                            100.0,
                            "3306",
                        )
                        .changed()
                        {
                            self.ui.pf_error = None;
                        }
                    });

                    ui.add_space(6.0);
                    ui.label("Xray 出站节点");
                    let selected_outbound_label = outbounds
                        .iter()
                        .find(|(tag, _)| *tag == self.ui.pf_form.outbound_tag)
                        .map(|(_, label)| format!("Xray · {label}"))
                        .unwrap_or_else(|| self.ui.pf_form.outbound_tag.clone());
                    egui::ComboBox::from_id_salt("pf-outbound-select")
                        .width((ui.available_width() - 16.0).clamp(220.0, 520.0))
                        .selected_text(selected_outbound_label)
                        .show_ui(ui, |ui| {
                            for (tag, label) in &outbounds {
                                let is_sel = self.ui.pf_form.outbound_tag == *tag;
                                if ui.selectable_label(is_sel, format!("Xray · {label}")).clicked()
                                {
                                    self.ui.pf_form.outbound_tag = tag.clone();
                                    self.ui.pf_error = None;
                                }
                            }
                        });

                    let outbound_is_direct = parsed_json
                        .as_ref()
                        .and_then(|v| v.get("outbounds"))
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|arr| {
                            arr.iter().any(|o| {
                                o.get("tag").and_then(|t| t.as_str())
                                    == Some(self.ui.pf_form.outbound_tag.as_str())
                                    && o.get("protocol").and_then(|p| p.as_str()) == Some("freedom")
                            })
                        });
                    if !outbound_is_direct
                        && crate::port_forward::is_private_or_loopback_host(
                            &self.ui.pf_form.target_host,
                        )
                    {
                        ui.add_space(4.0);
                        crate::widgets::banner_box(
                            ui,
                            crate::widgets::BadgeTone::Accent,
                            "目标为内网/回环地址且出口走代理节点：请确认远端 Xray 服务端未拦截 geoip:private。",
                        );
                    }
                }

                let live_conflict = crate::port_forward::find_local_port_conflict(
                    parsed_json.as_ref(),
                    &self.ui.pf_form,
                    &ssh_forwards,
                );
                let form_err = self.ui.pf_error.clone().or_else(|| {
                    crate::port_forward::check_form_error_with_optional_config(
                        parsed_json.as_ref(),
                        &self.ui.pf_form,
                        &ssh_hosts,
                        &ssh_forwards,
                    )
                    .filter(|err| live_conflict.as_ref() != Some(err))
                });
                if let Some(err) = form_err {
                    ui.add_space(4.0);
                    crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Danger, &err);
                }
            });

        if let Some(maybe_hint) = open_host_modal {
            let mut host_form = ui::SshHostForm::for_new(&self.ui.ssh_state.credentials, true);
            if let Some(hint) = maybe_hint {
                if !hint.host.is_empty() {
                    host_form.host = hint.host;
                }
                if hint.port > 0 {
                    host_form.port = hint.port.to_string();
                }
                if !hint.user.is_empty() {
                    host_form.cred_mode = ui::SshHostCredMode::Inline;
                    host_form.inline_username = hint.user;
                }
            }
            self.ui.ssh_host_form = host_form;
            self.ui.dialog = Some(Dialog::SshHost);
            return true;
        }

        dialog_actions(ui);
        let mut keep_open = true;
        let can_save = crate::port_forward::check_form_error_with_optional_config(
            parsed_json.as_ref(),
            &self.ui.pf_form,
            &self.ui.ssh_state.hosts,
            &self.ui.ssh_state.forwards,
        )
        .is_none();
        let enter_save = can_save && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal_wrapped(|ui| {
            if self.ui.pf_form.outbound_is_ssh {
                ui.checkbox(&mut self.ui.pf_form.start_now, "保存并立即启动");
            }
            if ui.button("取消").clicked() {
                self.ui.pf_error = None;
                keep_open = false;
            }
            let save_label = if self.ui.pf_form.outbound_is_ssh {
                if self.ui.pf_form.start_now {
                    "保存并启动"
                } else {
                    "保存 SSH 隧道"
                }
            } else {
                "保存草稿"
            };
            if primary(ui, save_label, can_save) || enter_save {
                let form = self.ui.pf_form.clone();
                if form.outbound_is_ssh {
                    if (form.editing_tag.is_some() || form.editing_paused_index.is_some())
                        && let Some(ref mut value) = parsed_json
                    {
                        let entries = crate::port_forward::extract_entries(
                            value,
                            &self.ui.workspace,
                            &self.profile_id,
                        );
                        if let Some(old) = entries.iter().find(|e| {
                            (form.editing_tag.is_some()
                                && form.editing_tag.as_deref() == Some(e.tag.as_str()))
                                || (form.editing_paused_index.is_some()
                                    && form.editing_paused_index == e.paused_index)
                        }) {
                            crate::port_forward::remove_entry(
                                value,
                                &mut self.ui.workspace,
                                &self.profile_id,
                                old,
                            );
                            self.save_workspace();
                            if old.enabled {
                                self.editor = serde_json::to_string_pretty(value).unwrap();
                                self.draft_edited();
                            }
                        }
                    }
                    let forward = crate::port_forward::ssh_forward_from_form(&form);
                    self.send(Command::SaveSshForward {
                        forward,
                        start_now: form.start_now,
                    });
                    self.ui.pf_error = None;
                    keep_open = false;
                } else if let Some(ref mut value) = parsed_json {
                    match crate::port_forward::save_entry(
                        value,
                        &mut self.ui.workspace,
                        &self.profile_id,
                        &form,
                    ) {
                        Ok(tag) => {
                            self.save_workspace();
                            if form.editing_paused_index.is_none() {
                                self.editor = serde_json::to_string_pretty(value).unwrap();
                                self.draft_edited();
                            }
                            if let Some(old_ssh_id) = form.editing_ssh_id {
                                self.send(Command::DeleteSshForward { id: old_ssh_id });
                            }
                            self.ui.pf_error = None;
                            self.message = format!("已保存端口转发：{tag}");
                            keep_open = false;
                        }
                        Err(err) => {
                            self.ui.pf_error = Some(err);
                        }
                    }
                } else {
                    self.ui.pf_error =
                        Some("当前 Xray 草稿存在 JSON 语法错误，无法保存 Xray 转发。".into());
                }
            }
        });
        keep_open
    }

    fn ssh_credential_dialog_contents(&mut self, ui: &mut egui::Ui) -> bool {
        let editing = self.ui.ssh_cred_form.editing_id.is_some();
        dialog_title(
            ui,
            if editing {
                "编辑 SSH 凭证"
            } else {
                "新建 SSH 凭证"
            },
            "账号与密码/口令保存在受保护的数据目录（Windows 使用当前用户 DPAPI 加密），不会在界面或日志中回显。",
        );
        egui::ScrollArea::vertical()
            .id_salt("ssh-credential-dialog-body")
            .max_height((ui.ctx().content_rect().height() - 250.0).max(160.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label("SSH 账号（用户名）");
                crate::widgets::singleline_select_all(
                    ui,
                    &mut self.ui.ssh_cred_form.username,
                    ui.available_width(),
                    "例如：ops / root / ubuntu",
                );
                ui.add_space(6.0);
                ui.label("凭证名称（可选，留空自动生成）");
                let auto_cred_hint = format!(
                    "留空自动命名：{}",
                    crate::port_forward::auto_ssh_credential_name(
                        &self.ui.ssh_cred_form.username,
                        self.ui.ssh_cred_form.auth_kind
                    )
                );
                crate::widgets::singleline_select_all(
                    ui,
                    &mut self.ui.ssh_cred_form.name,
                    ui.available_width(),
                    &auto_cred_hint,
                );
                ui.add_space(6.0);
                ui.label("认证方式");
                ui.horizontal_wrapped(|ui| {
                    for (kind, label) in [
                        (SshAuthKind::Password, "账号密码"),
                        (SshAuthKind::PrivateKey, "私钥文件"),
                    ] {
                        if crate::widgets::seg_button(
                            ui,
                            label,
                            self.ui.ssh_cred_form.auth_kind == kind,
                            true,
                        )
                        .clicked()
                        {
                            self.ui.ssh_cred_form.auth_kind = kind;
                            self.ui.ssh_cred_form.error = None;
                        }
                    }
                });
                ui.add_space(6.0);
                match self.ui.ssh_cred_form.auth_kind {
                    SshAuthKind::Password => {
                        ui.label(if editing && self.ui.ssh_cred_form.has_existing_password {
                            "登录密码（留空则保留当前已加密保存的密码）"
                        } else {
                            "登录密码"
                        });
                        ui.add_sized(
                            [ui.available_width(), crate::widgets::CONTROL_HEIGHT],
                            egui::TextEdit::singleline(&mut self.ui.ssh_cred_form.password)
                                .password(!self.ui.ssh_cred_form.show_secret)
                                .hint_text(
                                    if editing && self.ui.ssh_cred_form.has_existing_password {
                                        "已保存（留空不变）"
                                    } else {
                                        "输入 SSH 密码"
                                    },
                                ),
                        );
                        ui.checkbox(&mut self.ui.ssh_cred_form.show_secret, "临时显示输入字符");
                    }
                    SshAuthKind::PrivateKey => {
                        ui.label("私钥文件路径（OpenSSH / PEM 格式，支持相对 data 目录路径）");
                        ui.horizontal_wrapped(|ui| {
                            let path_w = (ui.available_width() - 96.0).max(160.0);
                            crate::widgets::singleline_select_all(
                                ui,
                                &mut self.ui.ssh_cred_form.private_key_path,
                                path_w,
                                "例如：keys/id_ed25519",
                            );
                            #[cfg(windows)]
                            if ui.button("选择文件…").clicked()
                                && let Some(path) = rfd::FileDialog::new().pick_file()
                            {
                                self.ui.ssh_cred_form.private_key_path =
                                    proxy_model::to_portable_string(
                                        &self.root,
                                        &path.to_string_lossy(),
                                    );
                            }
                        });
                        ui.add_space(6.0);
                        ui.label(
                            if editing && self.ui.ssh_cred_form.has_existing_passphrase {
                                "私钥口令 Passphrase（可选；留空保留原口令）"
                            } else {
                                "私钥口令 Passphrase（无加密可留空）"
                            },
                        );
                        ui.add_sized(
                            [ui.available_width(), crate::widgets::CONTROL_HEIGHT],
                            egui::TextEdit::singleline(&mut self.ui.ssh_cred_form.passphrase)
                                .password(!self.ui.ssh_cred_form.show_secret)
                                .hint_text("可选"),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut self.ui.ssh_cred_form.show_secret, "临时显示口令");
                            if editing && self.ui.ssh_cred_form.has_existing_passphrase {
                                ui.checkbox(
                                    &mut self.ui.ssh_cred_form.clear_passphrase,
                                    "清除已保存口令",
                                );
                            }
                        });
                    }
                }
                if let Some(err) = &self.ui.ssh_cred_form.error {
                    ui.add_space(4.0);
                    crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Danger, err);
                }
            });

        dialog_actions(ui);
        let f = &self.ui.ssh_cred_form;
        let valid = !f.username.trim().is_empty()
            && match f.auth_kind {
                SshAuthKind::Password => {
                    !f.password.is_empty() || (editing && f.has_existing_password)
                }
                SshAuthKind::PrivateKey => !f.private_key_path.trim().is_empty(),
            };
        let mut keep_open = true;
        ui.horizontal_wrapped(|ui| {
            if ui.button("取消").clicked() {
                if self.ui.ssh_cred_form.return_to_host {
                    self.ui.ssh_cred_form.return_to_host = false;
                    self.ui.dialog = Some(Dialog::SshHost);
                } else {
                    keep_open = false;
                }
            }
            if primary(ui, "保存凭证", valid && !self.busy) {
                let form = &self.ui.ssh_cred_form;
                let cred_name = if form.name.trim().is_empty() {
                    crate::port_forward::auto_ssh_credential_name(&form.username, form.auth_kind)
                } else {
                    form.name.trim().to_owned()
                };
                let password = match form.auth_kind {
                    SshAuthKind::Password => {
                        if form.password.is_empty() {
                            None
                        } else {
                            Some(form.password.clone())
                        }
                    }
                    SshAuthKind::PrivateKey => None,
                };
                let passphrase = if form.auth_kind == SshAuthKind::PrivateKey {
                    if form.clear_passphrase {
                        Some(String::new())
                    } else if form.passphrase.is_empty() {
                        None
                    } else {
                        Some(form.passphrase.clone())
                    }
                } else {
                    None
                };
                self.send(Command::SaveSshCredential {
                    credential: SshCredentialInput {
                        id: form.editing_id.clone().unwrap_or_default(),
                        name: cred_name,
                        username: form.username.trim().to_owned(),
                        auth_kind: form.auth_kind,
                        password,
                        private_key_path: form.private_key_path.trim().into(),
                        passphrase,
                    },
                });
                self.ui.ssh_cred_form.password.clear();
                self.ui.ssh_cred_form.passphrase.clear();
                keep_open = false;
            }
        });
        keep_open
    }

    fn ssh_host_dialog_contents(&mut self, ui: &mut egui::Ui) -> bool {
        let editing = self.ui.ssh_host_form.editing_id.is_some();
        dialog_title(
            ui,
            if editing {
                "编辑 SSH 跳板机"
            } else {
                "新建 SSH 跳板机"
            },
            "配置跳板机地址、选择或直接填写登录凭证，并固定服务器 SHA256 Host Key 指纹。",
        );
        let credentials = self.ui.ssh_state.credentials.clone();
        if self.ui.ssh_host_form.cred_mode == ui::SshHostCredMode::Reuse
            && self.ui.ssh_host_form.credential_id.is_empty()
            && let Some(first) = credentials.first()
        {
            self.ui.ssh_host_form.credential_id = first.id.clone();
        }
        egui::ScrollArea::vertical()
            .id_salt("ssh-host-dialog-body")
            .max_height((ui.ctx().content_rect().height() - 250.0).max(160.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label("服务器地址");
                    let host_w = (ui.available_width() - 210.0).max(160.0);
                    let prev_len = self.ui.ssh_host_form.host.len();
                    let addr_resp = crate::widgets::singleline_select_all(
                        ui,
                        &mut self.ui.ssh_host_form.host,
                        host_w,
                        "bastion.example.com 或 ops@bastion.corp:2222",
                    );
                    if addr_resp.changed() {
                        self.ui.ssh_host_form.error = None;
                        self.ui.ssh_host_form.test_result = None;
                    }
                    let pasted_or_blurred = (addr_resp.changed()
                        && self.ui.ssh_host_form.host.len() > prev_len.saturating_add(1))
                        || addr_resp.lost_focus();
                    if pasted_or_blurred
                        && let Some((maybe_user, clean_host, maybe_port)) =
                            crate::port_forward::parse_ssh_host_paste(&self.ui.ssh_host_form.host)
                    {
                        self.ui.ssh_host_form.host = clean_host;
                        if let Some(port) = maybe_port {
                            self.ui.ssh_host_form.port = port.to_string();
                        }
                        if let Some(user) = maybe_user
                            && self.ui.ssh_host_form.cred_mode == ui::SshHostCredMode::Inline
                        {
                            self.ui.ssh_host_form.inline_username = user;
                        }
                    }
                    ui.label("SSH 端口");
                    if crate::widgets::singleline_select_all(
                        ui,
                        &mut self.ui.ssh_host_form.port,
                        84.0,
                        "22",
                    )
                    .changed()
                    {
                        self.ui.ssh_host_form.error = None;
                        self.ui.ssh_host_form.test_result = None;
                    }
                });
                ui.add_space(6.0);
                ui.label("跳板机名称（可选，留空自动使用服务器地址）");
                let host_name_hint = if self.ui.ssh_host_form.host.trim().is_empty() {
                    "留空自动使用服务器地址".into()
                } else {
                    format!("留空自动命名：{}", self.ui.ssh_host_form.host.trim())
                };
                crate::widgets::singleline_select_all(
                    ui,
                    &mut self.ui.ssh_host_form.name,
                    ui.available_width(),
                    &host_name_hint,
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label("SSH 登录凭证");
                    let has_creds = !credentials.is_empty();
                    let reuse_resp = crate::widgets::seg_button(
                        ui,
                        "复用已有凭证",
                        self.ui.ssh_host_form.cred_mode == ui::SshHostCredMode::Reuse,
                        has_creds,
                    );
                    if !has_creds {
                        reuse_resp.on_hover_text("当前暂无已保存凭证，请直接填写新凭证");
                    } else if reuse_resp.clicked() {
                        self.ui.ssh_host_form.cred_mode = ui::SshHostCredMode::Reuse;
                        if self.ui.ssh_host_form.credential_id.is_empty()
                            && let Some(first) = credentials.first()
                        {
                            self.ui.ssh_host_form.credential_id = first.id.clone();
                        }
                        self.ui.ssh_host_form.error = None;
                        self.ui.ssh_host_form.test_result = None;
                    }
                    if crate::widgets::seg_button(
                        ui,
                        "直接填写新凭证",
                        self.ui.ssh_host_form.cred_mode == ui::SshHostCredMode::Inline,
                        true,
                    )
                    .clicked()
                    {
                        self.ui.ssh_host_form.cred_mode = ui::SshHostCredMode::Inline;
                        self.ui.ssh_host_form.error = None;
                        self.ui.ssh_host_form.test_result = None;
                    }
                });
                match self.ui.ssh_host_form.cred_mode {
                    ui::SshHostCredMode::Reuse => {
                        let selected_cred_label = credentials
                            .iter()
                            .find(|c| c.id == self.ui.ssh_host_form.credential_id)
                            .map(|c| format!("{} (账号：{})", c.name, c.username))
                            .unwrap_or_else(|| "请选择 SSH 凭证".into());
                        egui::ComboBox::from_id_salt("ssh-host-cred-select")
                            .width((ui.available_width() - 16.0).clamp(200.0, 520.0))
                            .selected_text(selected_cred_label)
                            .show_ui(ui, |ui| {
                                for cred in &credentials {
                                    ui.selectable_value(
                                        &mut self.ui.ssh_host_form.credential_id,
                                        cred.id.clone(),
                                        format!("{} (账号：{})", cred.name, cred.username),
                                    );
                                }
                            });
                    }
                    ui::SshHostCredMode::Inline => {
                        crate::widgets::subcard(ui, None, true, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("认证方式").small());
                                for (kind, label) in [
                                    (SshAuthKind::Password, "账号密码"),
                                    (SshAuthKind::PrivateKey, "私钥文件"),
                                ] {
                                    if crate::widgets::seg_button(
                                        ui,
                                        label,
                                        self.ui.ssh_host_form.inline_auth_kind == kind,
                                        true,
                                    )
                                    .clicked()
                                    {
                                        self.ui.ssh_host_form.inline_auth_kind = kind;
                                        self.ui.ssh_host_form.error = None;
                                        self.ui.ssh_host_form.test_result = None;
                                    }
                                }
                            });
                            ui.add_space(4.0);
                            ui.label("SSH 账号");
                            if crate::widgets::singleline_select_all(
                                ui,
                                &mut self.ui.ssh_host_form.inline_username,
                                ui.available_width(),
                                "例如：ops / root",
                            )
                            .changed()
                            {
                                self.ui.ssh_host_form.test_result = None;
                            }
                            ui.add_space(4.0);
                            ui.label("凭证名称（可选，留空自动生成）");
                            let inline_cred_hint = format!(
                                "留空自动命名：{}",
                                crate::port_forward::auto_ssh_credential_name(
                                    &self.ui.ssh_host_form.inline_username,
                                    self.ui.ssh_host_form.inline_auth_kind
                                )
                            );
                            crate::widgets::singleline_select_all(
                                ui,
                                &mut self.ui.ssh_host_form.inline_cred_name,
                                ui.available_width(),
                                &inline_cred_hint,
                            );
                            ui.add_space(4.0);
                            match self.ui.ssh_host_form.inline_auth_kind {
                                SshAuthKind::Password => {
                                    ui.label("登录密码");
                                    ui.add_sized(
                                        [ui.available_width(), crate::widgets::CONTROL_HEIGHT],
                                        egui::TextEdit::singleline(
                                            &mut self.ui.ssh_host_form.inline_password,
                                        )
                                        .password(!self.ui.ssh_host_form.inline_show_secret)
                                        .hint_text("仅本机 DPAPI 加密存储，不回显"),
                                    );
                                    ui.checkbox(
                                        &mut self.ui.ssh_host_form.inline_show_secret,
                                        "临时显示输入字符",
                                    );
                                }
                                SshAuthKind::PrivateKey => {
                                    ui.label("私钥文件路径（OpenSSH / PEM 格式）");
                                    ui.horizontal_wrapped(|ui| {
                                        let path_w = (ui.available_width() - 96.0).max(160.0);
                                        crate::widgets::singleline_select_all(
                                            ui,
                                            &mut self.ui.ssh_host_form.inline_private_key_path,
                                            path_w,
                                            "例如：keys/id_ed25519",
                                        );
                                        #[cfg(windows)]
                                        if ui.button("选择文件…").clicked()
                                            && let Some(path) = rfd::FileDialog::new().pick_file()
                                        {
                                            self.ui.ssh_host_form.inline_private_key_path =
                                                proxy_model::to_portable_string(
                                                    &self.root,
                                                    &path.to_string_lossy(),
                                                );
                                        }
                                    });
                                    ui.add_space(4.0);
                                    ui.label("私钥口令 Passphrase（无加密可留空）");
                                    ui.add_sized(
                                        [ui.available_width(), crate::widgets::CONTROL_HEIGHT],
                                        egui::TextEdit::singleline(
                                            &mut self.ui.ssh_host_form.inline_passphrase,
                                        )
                                        .password(!self.ui.ssh_host_form.inline_show_secret)
                                        .hint_text("可选"),
                                    );
                                    ui.checkbox(
                                        &mut self.ui.ssh_host_form.inline_show_secret,
                                        "临时显示口令",
                                    );
                                }
                            }
                            ui.label(
                                RichText::new("保存跳板机时一并 DPAPI 加密落盘，不回显明文。")
                                    .small()
                                    .weak(),
                            );
                        });
                    }
                }
                ui.add_space(6.0);
                ui.label("服务器 Host Key 指纹（SHA256，首次连接前请探测并核对）");
                let port_ok = self
                    .ui
                    .ssh_host_form
                    .port
                    .trim()
                    .parse::<u16>()
                    .ok()
                    .filter(|p| *p > 0);
                let inline_ready = match self.ui.ssh_host_form.inline_auth_kind {
                    SshAuthKind::Password => {
                        !self.ui.ssh_host_form.inline_username.trim().is_empty()
                            && !self.ui.ssh_host_form.inline_password.is_empty()
                    }
                    SshAuthKind::PrivateKey => {
                        !self.ui.ssh_host_form.inline_username.trim().is_empty()
                            && !self
                                .ui
                                .ssh_host_form
                                .inline_private_key_path
                                .trim()
                                .is_empty()
                    }
                };
                let cred_ready = match self.ui.ssh_host_form.cred_mode {
                    ui::SshHostCredMode::Reuse => credentials
                        .iter()
                        .any(|c| c.id == self.ui.ssh_host_form.credential_id),
                    ui::SshHostCredMode::Inline => inline_ready,
                };
                let mut trigger_test = false;
                ui.horizontal_wrapped(|ui| {
                    let fp_w = (ui.available_width() - 235.0).max(150.0);
                    if crate::widgets::singleline_select_all(
                        ui,
                        &mut self.ui.ssh_host_form.host_key_fingerprint,
                        fp_w,
                        "SHA256:...",
                    )
                    .changed()
                    {
                        self.ui.ssh_host_form.test_result = None;
                    }
                    let can_probe = !self.ui.ssh_host_form.host.trim().is_empty()
                        && port_ok.is_some()
                        && !self.ui.ssh_host_form.probing;
                    let probe_label = if self.ui.ssh_host_form.probing {
                        "探测中…"
                    } else {
                        "探测指纹"
                    };
                    if ui
                        .add_enabled(can_probe, egui::Button::new(probe_label))
                        .clicked()
                    {
                        self.ui.ssh_host_form.probing = true;
                        self.ui.ssh_host_form.error = None;
                        self.ui.ssh_host_form.probe_note = None;
                        self.send(Command::ProbeSshHostKey {
                            host: self.ui.ssh_host_form.host.trim().to_owned(),
                            port: port_ok.unwrap(),
                        });
                    }
                    let can_test = !self.ui.ssh_host_form.host.trim().is_empty()
                        && port_ok.is_some()
                        && cred_ready
                        && !self.ui.ssh_host_form.testing;
                    let test_label = if self.ui.ssh_host_form.testing {
                        "测试中…"
                    } else {
                        "测试连接"
                    };
                    if ui
                        .add_enabled(can_test, egui::Button::new(test_label))
                        .clicked()
                    {
                        trigger_test = true;
                    }
                });
                if trigger_test {
                    let host_addr = self.ui.ssh_host_form.host.trim().to_owned();
                    let host_name = if self.ui.ssh_host_form.name.trim().is_empty() {
                        host_addr.clone()
                    } else {
                        self.ui.ssh_host_form.name.trim().to_owned()
                    };
                    let inline_cred =
                        if self.ui.ssh_host_form.cred_mode == ui::SshHostCredMode::Inline {
                            Some(build_inline_cred_input(&self.ui.ssh_host_form))
                        } else {
                            None
                        };
                    self.ui.ssh_host_form.testing = true;
                    self.ui.ssh_host_form.test_result = None;
                    self.ui.ssh_host_form.error = None;
                    self.send(Command::TestSshHost {
                        host_id: String::new(),
                        host: Some(SshHost {
                            id: String::new(),
                            name: host_name,
                            host: host_addr,
                            port: port_ok.unwrap_or(proxy_model::DEFAULT_SSH_PORT),
                            credential_id: if inline_cred.is_none() {
                                self.ui.ssh_host_form.credential_id.clone()
                            } else {
                                String::new()
                            },
                            host_key_fingerprint: self
                                .ui
                                .ssh_host_form
                                .host_key_fingerprint
                                .trim()
                                .to_owned(),
                        }),
                        inline_credential: inline_cred,
                    });
                }
                if let Some(note) = &self.ui.ssh_host_form.probe_note {
                    ui.add_space(4.0);
                    crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Success, note);
                }
                if let Some((ok, ref msg)) = self.ui.ssh_host_form.test_result {
                    ui.add_space(4.0);
                    crate::widgets::banner_box(
                        ui,
                        if ok {
                            crate::widgets::BadgeTone::Success
                        } else {
                            crate::widgets::BadgeTone::Danger
                        },
                        msg,
                    );
                }
                if let Some(err) = &self.ui.ssh_host_form.error {
                    ui.add_space(4.0);
                    crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Danger, err);
                }
            });

        dialog_actions(ui);
        let port_num = self
            .ui
            .ssh_host_form
            .port
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0);
        let cred_valid = match self.ui.ssh_host_form.cred_mode {
            ui::SshHostCredMode::Reuse => credentials
                .iter()
                .any(|c| c.id == self.ui.ssh_host_form.credential_id),
            ui::SshHostCredMode::Inline => {
                !self.ui.ssh_host_form.inline_username.trim().is_empty()
                    && match self.ui.ssh_host_form.inline_auth_kind {
                        SshAuthKind::Password => !self.ui.ssh_host_form.inline_password.is_empty(),
                        SshAuthKind::PrivateKey => !self
                            .ui
                            .ssh_host_form
                            .inline_private_key_path
                            .trim()
                            .is_empty(),
                    }
            }
        };
        let valid =
            !self.ui.ssh_host_form.host.trim().is_empty() && port_num.is_some() && cred_valid;
        let mut keep_open = true;
        if self.ui.ssh_host_form.return_to_pf {
            ui.label(
                RichText::new("保存后将自动返回「端口转发」弹窗并选中此跳板机")
                    .small()
                    .weak(),
            );
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("取消").clicked() {
                self.ui.ssh_host_form.inline_password.clear();
                self.ui.ssh_host_form.inline_passphrase.clear();
                if self.ui.ssh_host_form.return_to_pf {
                    self.ui.ssh_host_form.return_to_pf = false;
                    self.ui.dialog = Some(Dialog::PortForward);
                } else {
                    keep_open = false;
                }
            }
            if primary(ui, "保存跳板机", valid && !self.busy) {
                let host_addr = self.ui.ssh_host_form.host.trim().to_owned();
                let host_name = if self.ui.ssh_host_form.name.trim().is_empty() {
                    host_addr.clone()
                } else {
                    self.ui.ssh_host_form.name.trim().to_owned()
                };
                let inline_credential =
                    if self.ui.ssh_host_form.cred_mode == ui::SshHostCredMode::Inline {
                        Some(build_inline_cred_input(&self.ui.ssh_host_form))
                    } else {
                        None
                    };
                self.send(Command::SaveSshHost {
                    host: SshHost {
                        id: self.ui.ssh_host_form.editing_id.clone().unwrap_or_default(),
                        name: host_name,
                        host: host_addr,
                        port: port_num.unwrap_or(proxy_model::DEFAULT_SSH_PORT),
                        credential_id: if inline_credential.is_none() {
                            self.ui.ssh_host_form.credential_id.clone()
                        } else {
                            String::new()
                        },
                        host_key_fingerprint: self
                            .ui
                            .ssh_host_form
                            .host_key_fingerprint
                            .trim()
                            .to_owned(),
                    },
                    inline_credential,
                });
                self.ui.ssh_host_form.inline_password.clear();
                self.ui.ssh_host_form.inline_passphrase.clear();
                keep_open = false;
            }
        });
        keep_open
    }
}

fn build_inline_cred_input(form: &ui::SshHostForm) -> SshCredentialInput {
    let name = if form.inline_cred_name.trim().is_empty() {
        crate::port_forward::auto_ssh_credential_name(&form.inline_username, form.inline_auth_kind)
    } else {
        form.inline_cred_name.trim().to_owned()
    };
    let password = match form.inline_auth_kind {
        SshAuthKind::Password => {
            (!form.inline_password.is_empty()).then(|| form.inline_password.clone())
        }
        SshAuthKind::PrivateKey => None,
    };
    let passphrase = match form.inline_auth_kind {
        SshAuthKind::PrivateKey => {
            (!form.inline_passphrase.is_empty()).then(|| form.inline_passphrase.clone())
        }
        SshAuthKind::Password => None,
    };
    SshCredentialInput {
        id: String::new(),
        name,
        username: form.inline_username.trim().to_owned(),
        auth_kind: form.inline_auth_kind,
        password,
        private_key_path: if form.inline_auth_kind == SshAuthKind::PrivateKey {
            form.inline_private_key_path.trim().into()
        } else {
            PathBuf::new()
        },
        passphrase,
    }
}

fn apply_parsed_ssh_cmd(form: &mut crate::port_forward::PortForwardForm, ssh_hosts: &[SshHost]) {
    match crate::port_forward::parse_ssh_l_command(&form.ssh_cmd_input) {
        Ok(parsed) => {
            form.listen = parsed.listen.clone();
            form.custom_listen = parsed.listen != crate::port_forward::LISTEN_LOCAL
                && parsed.listen != crate::port_forward::LISTEN_LAN;
            form.local_port = parsed.local_port.to_string();
            form.target_host = parsed.target_host.clone();
            form.target_port = parsed.target_port.to_string();
            form.outbound_is_ssh = true;
            form.network = "tcp".into();

            if let Some(ref jump_host) = parsed.jump_host {
                if let Some(matched_id) =
                    crate::port_forward::match_ssh_host_id(ssh_hosts, jump_host, parsed.ssh_port)
                {
                    let matched_name = ssh_hosts
                        .iter()
                        .find(|h| h.id == matched_id)
                        .map(|h| h.name.as_str())
                        .unwrap_or(jump_host.as_str());
                    form.ssh_host_id = matched_id;
                    form.parsed_ssh_hint = None;
                    form.ssh_cmd_feedback = Some((
                        false,
                        format!("已填入地址端口，并自动匹配跳板机「{matched_name}」。"),
                    ));
                } else {
                    let user_prefix = parsed
                        .user
                        .as_deref()
                        .map(|u| format!("{u}@"))
                        .unwrap_or_default();
                    form.parsed_ssh_hint = Some(crate::port_forward::ParsedSshJumpHint {
                        user: parsed.user.unwrap_or_default(),
                        host: jump_host.clone(),
                        port: parsed.ssh_port,
                    });
                    form.ssh_cmd_feedback = Some((
                        false,
                        format!(
                            "已填入地址端口；跳板机 {user_prefix}{jump_host}:{} 尚未登记，可点下方按钮一键新建。",
                            parsed.ssh_port
                        ),
                    ));
                }
            } else {
                form.parsed_ssh_hint = None;
                form.ssh_cmd_feedback = Some((
                    false,
                    "已填入本地端口与目标地址端口，请选择 SSH 跳板机。".into(),
                ));
            }
        }
        Err(err) => {
            form.parsed_ssh_hint = None;
            form.ssh_cmd_feedback = Some((true, err));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_listen_scope_and_port(
    ui: &mut egui::Ui,
    form: &mut crate::port_forward::PortForwardForm,
    pf_error: &mut Option<String>,
    parsed_json: Option<&serde_json::Value>,
    paused_pf: &[crate::workspace::PausedPortForward],
    ssh_forwards: &[SshForward],
    lan_warn_text: &str,
) {
    ui.label("本机监听范围与端口");
    let mut focus_custom_listen = false;
    ui.horizontal_wrapped(|ui| {
        let is_local = form.listen.trim() == crate::port_forward::LISTEN_LOCAL;
        let is_lan = form.listen.trim() == crate::port_forward::LISTEN_LAN;
        let is_custom = form.custom_listen || (!is_local && !is_lan);
        if crate::widgets::seg_button(ui, "仅本机 (127.0.0.1)", is_local && !is_custom, true)
            .clicked()
        {
            form.listen = crate::port_forward::LISTEN_LOCAL.into();
            form.custom_listen = false;
            *pf_error = None;
        }
        if crate::widgets::seg_button(ui, "局域网可访问 (0.0.0.0)", is_lan && !is_custom, true)
            .clicked()
        {
            form.listen = crate::port_forward::LISTEN_LAN.into();
            form.custom_listen = false;
            *pf_error = None;
        }
        if crate::widgets::seg_button(ui, "自定义 IP", is_custom, true).clicked() {
            form.custom_listen = true;
            focus_custom_listen = true;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("监听 IP");
        let ip_w = (ui.available_width() - 315.0).max(120.0);
        let listen_resp =
            crate::widgets::singleline_select_all(ui, &mut form.listen, ip_w, "127.0.0.1");
        if focus_custom_listen {
            crate::widgets::focus_and_select_all(ui.ctx(), &listen_resp, &form.listen);
        }
        if listen_resp.changed() {
            let trimmed = form.listen.trim();
            form.custom_listen = trimmed != crate::port_forward::LISTEN_LOCAL
                && trimmed != crate::port_forward::LISTEN_LAN;
            *pf_error = None;
        }
        ui.label("本机端口");
        if crate::widgets::singleline_select_all(ui, &mut form.local_port, 90.0, "15432").changed()
        {
            *pf_error = None;
        }
        let target_port_ok = form
            .target_port
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0);
        if ui
            .add_enabled(target_port_ok.is_some(), egui::Button::new("同目标端口"))
            .clicked()
            && let Some(tp) = target_port_ok
        {
            form.local_port = tp.to_string();
            *pf_error = None;
        }
    });
    if !crate::port_forward::is_loopback_listen(&form.listen)
        && form.listen.trim().parse::<std::net::IpAddr>().is_ok()
    {
        ui.add_space(4.0);
        crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Warn, lan_warn_text);
    }
    if let Some(conflict_msg) =
        crate::port_forward::find_local_port_conflict(parsed_json, form, ssh_forwards)
    {
        let cur = form.local_port.trim().parse::<u16>().unwrap_or(15432);
        let free_port = crate::port_forward::next_free_port_all(
            cur.saturating_add(1),
            parsed_json,
            paused_pf,
            ssh_forwards,
            form.editing_tag.as_deref(),
            form.editing_ssh_id.as_deref(),
        );
        ui.add_space(4.0);
        crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Danger, &conflict_msg);
        if crate::widgets::compact_chip(ui, &format!("改用空闲端口 ({free_port})"), true).clicked()
        {
            form.local_port = free_port.to_string();
            *pf_error = None;
        }
    }
}

/// Explain coverage limitations without changing the persisted backend evidence.
fn resource_note(note: &str) -> String {
    if let Some(path) = note
        .strip_prefix("Uncovered dependency capability ")
        .and_then(|s| s.strip_suffix("; automatic recovery disabled"))
    {
        format!("尚未覆盖字段：{path}")
    } else {
        match note {
            "Automatic recovery covers HTTP/SOCKS inbounds only"
            | "Automatic recovery covers HTTP/SOCKS/Mixed inbounds only"
            | "Automatic recovery covers HTTP/SOCKS/Mixed/dokodemo-door inbounds only" => {
                "自动恢复目前仅覆盖 HTTP / SOCKS / Mixed / dokodemo-door 入站。".into()
            }
            "Outbound protocol dependency coverage is incomplete" => {
                "此出站协议的依赖覆盖尚不完整。".into()
            }
            _ => note.into(),
        }
    }
}
