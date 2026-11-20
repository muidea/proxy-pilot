use super::*;

impl App {
    fn profile_port_forward_count(&self, profile_id: &str) -> usize {
        let json_text = if profile_id == self.profile_id {
            Some(self.editor.as_str())
        } else if let Some(draft) = self.ui.drafts.get(profile_id) {
            Some(draft.json.as_str())
        } else {
            self.ui
                .profile_cache
                .get(profile_id)
                .map(|p| p.json.as_str())
        };
        json_text
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
            .map(|v| crate::port_forward::dokodemo_tags(&v).len())
            .unwrap_or(0)
    }
    pub(in crate::app) fn profiles_page(&mut self, ui: &mut egui::Ui, viewport_height: f32) {
        let page_top = ui.cursor().top();
        heading(
            ui,
            "配置",
            "编辑内容自动保存，不影响当前运行中的 Core；点击「检查并应用」校验并切换。",
        );
        let enabled = self.write_blocker().is_none();
        if ui.available_width() >= 780.0 {
            let picker_width = 216.0;
            let height = (viewport_height - (ui.cursor().top() - page_top)).max(400.0);
            let editor_width = ui.available_width() - picker_width - ui.spacing().item_spacing.x;
            ui.horizontal_top(|ui| {
                let picker = ui.allocate_ui_with_layout(
                    egui::vec2(picker_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.profile_picker(ui, enabled, true),
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(editor_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.profile_editor(ui, enabled, picker.response.rect.height()),
                );
            });
        } else {
            self.profile_picker(ui, enabled, false);
            ui.add_space(16.0);
            let height = (viewport_height - (ui.cursor().top() - page_top)).max(400.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.profile_editor(ui, enabled, 0.0),
            );
        }
    }
    fn profile_picker(&mut self, ui: &mut egui::Ui, enabled: bool, vertical: bool) {
        card(ui, |ui| {
            ui.label(
                RichText::new(format!("我的配置 · {}", self.profiles.len()))
                    .small()
                    .weak(),
            );
            if vertical && !self.profiles.is_empty() {
                egui::ScrollArea::vertical()
                    .max_height(430.0)
                    .id_salt("profile-list")
                    .show(ui, |ui| {
                        for profile in self.profiles.clone() {
                            self.profile_choice(ui, profile);
                        }
                    });
            } else if !self.profiles.is_empty() {
                egui::ScrollArea::horizontal()
                    .id_salt("profile-strip")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for profile in self.profiles.clone() {
                                self.profile_choice(ui, profile);
                            }
                        });
                    });
            }
            if vertical {
                self.profile_actions(ui, enabled, ui.available_width());
            } else {
                ui.horizontal(|ui| self.profile_actions(ui, enabled, 80.0));
            }
        });
    }
    fn profile_actions(&mut self, ui: &mut egui::Ui, enabled: bool, width: f32) {
        for label in ["新建", "导入", "刷新"] {
            if ui
                .add_enabled(
                    enabled || label == "刷新",
                    egui::Button::new(label).min_size(egui::vec2(width, CONTROL_HEIGHT)),
                )
                .clicked()
            {
                match label {
                    "新建" => self.begin_create(false, false),
                    "导入" => self.begin_create(true, false),
                    _ => {
                        self.refresh_profiles();
                        if !self.profile_id.is_empty() {
                            self.send(Command::GetProfile {
                                profile_id: self.profile_id.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    fn profile_choice(&mut self, ui: &mut egui::Ui, p: ProfileSummary) {
        let dirty = if p.id == self.profile_id {
            self.has_draft_edits()
        } else {
            self.ui.drafts.contains_key(&p.id)
        };
        let running = self.committed_id() == Some(p.id.as_str()) && self.is_running();
        let pending_apply = p.id == self.profile_id && self.has_unapplied_runtime_diff();
        let pf_count = self.profile_port_forward_count(&p.id);
        let remembered_mode = self.proxy_mode_for(&p.id);
        let mode_short = match remembered_mode {
            ProxyMode::Direct => "直连",
            ProxyMode::Pac => "PAC",
            ProxyMode::Global => "全局",
        };
        let selected = self.selected_profile == p.id;
        let title = self.ui.workspace.name(&p.id).to_owned();
        let meta = format!(
            "{} · {}{}{}",
            if running { "运行中" } else { "未运行" },
            mode_short,
            if dirty {
                " · 未落盘"
            } else if pending_apply {
                " · 待应用"
            } else {
                ""
            },
            if pf_count > 0 {
                format!(" · {pf_count} 条转发")
            } else {
                String::new()
            }
        );
        let fill = if selected {
            crate::widgets::accent_soft_color(ui)
        } else {
            ui.visuals().window_fill()
        };
        let stroke = egui::Stroke::new(
            if selected { 1.4 } else { 1.0 },
            if selected {
                crate::widgets::accent_color(ui)
            } else {
                crate::widgets::border_color(ui)
            },
        );
        let card_width = ui.available_width().clamp(140.0, 184.0);
        let frame_resp = egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_min_width(card_width - 22.0);
                ui.set_max_width(card_width - 22.0);
                ui.add(egui::Label::new(RichText::new(title).strong().size(13.5)).truncate());
                ui.add(egui::Label::new(RichText::new(meta).small().weak()).truncate());
            });
        let click_resp = ui
            .interact(
                frame_resp.response.rect,
                ui.make_persistent_id(("profile-item", &p.id)),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if click_resp.clicked() && self.selected_profile != p.id {
            if self.ui.draft_conflict || self.ui.unreadable_drafts.contains(&self.profile_id) {
                self.ui.dialog = Some(Dialog::Confirm(Action::Switch(p.id)));
            } else {
                self.switch_profile(p.id);
            }
        }
        ui.add_space(4.0);
    }
    fn profile_editor(&mut self, ui: &mut egui::Ui, enabled: bool, min_height: f32) {
        if self.profile_id.is_empty() {
            card_with_min_height(ui, min_height, |ui| {
                ui.label(RichText::new("添加第一份配置").strong().size(20.0));
                ui.label("选择场景、粘贴分享链接或导入已有 JSON。保存不会自动启动 Core。");
            });
            return;
        }
        card_with_min_height(ui, min_height, |ui| {
            ui.horizontal_wrapped(|ui| {
                if let Some(mut name) = self.ui.inline_rename.clone() {
                    let response = ui.add(singleline(&mut name, 220.0).char_limit(80));
                    let enter =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("确定").clicked() || enter {
                        if !name.trim().is_empty() {
                            self.ui.workspace.names.insert(
                                self.profile_id.clone(),
                                name.trim().chars().take(80).collect(),
                            );
                            self.save_workspace();
                        }
                        self.ui.inline_rename = None;
                    } else if ui.button("取消").clicked() {
                        self.ui.inline_rename = None;
                    } else {
                        self.ui.inline_rename = Some(name);
                    }
                } else {
                    ui.label(
                        RichText::new(self.ui.workspace.name(&self.profile_id))
                            .strong()
                            .size(21.0),
                    );
                    if ui.button("改名").clicked() {
                        self.ui.inline_rename =
                            Some(self.ui.workspace.name(&self.profile_id).into());
                    }
                }
                let (status_text, status_tone) = if self.has_draft_edits() {
                    ("未落盘修改", crate::widgets::BadgeTone::Warn)
                } else if self.has_unapplied_runtime_diff() {
                    ("已保存 · 待应用", crate::widgets::BadgeTone::Warn)
                } else if self.committed_id() == Some(self.profile_id.as_str()) && self.is_running()
                {
                    ("运行中", crate::widgets::BadgeTone::Success)
                } else {
                    ("已保存", crate::widgets::BadgeTone::Muted)
                };
                crate::widgets::badge(ui, status_text, status_tone);
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        enabled && self.ui.number_inputs.is_empty(),
                        egui::Button::new("复制"),
                    )
                    .on_hover_text("请先补全未完成的数值输入，再复制配置")
                    .clicked()
                {
                    self.begin_create(false, true);
                }
                if ui
                    .add_enabled(
                        enabled
                            && self.selected_profile == self.profile_id
                            && self.committed_id() != Some(self.profile_id.as_str()),
                        egui::Button::new("删除"),
                    )
                    .on_hover_text("上次已应用的配置仍被运行记录引用，需先应用另一份配置")
                    .clicked()
                {
                    self.ui.dialog = Some(Dialog::Confirm(Action::Delete {
                        profile: self.profile_id.clone(),
                        revision: self.revision.clone(),
                    }));
                }
                #[cfg(windows)]
                if ui
                    .add_enabled(
                        self.ui.number_inputs.is_empty(),
                        egui::Button::new("导出 JSON"),
                    )
                    .on_hover_text("请先补全未完成的数值输入，再导出 JSON；输入已随草稿保留")
                    .clicked()
                {
                    self.ui.dialog = Some(Dialog::Confirm(Action::ExportJson));
                }
            });
            if self.ui.draft_conflict {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "本地未落盘修改基于旧版本。请先确认是保留本地内容还是放弃未保存修改。",
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button("保留本地内容").clicked() {
                        self.ui.dialog =
                            Some(Dialog::Confirm(Action::AdoptDraft(self.profile_id.clone())));
                    }
                    if ui.button("放弃未保存修改").clicked() {
                        self.ui.dialog =
                            Some(Dialog::Confirm(Action::Discard(self.profile_id.clone())));
                    }
                });
            }
            ui.separator();
            let pf_count = self.profile_port_forward_count(&self.profile_id);
            let pf_title = if pf_count > 0 {
                format!("端口转发 ({pf_count})")
            } else {
                "端口转发".into()
            };
            if ui.available_width() >= 760.0 {
                let mut next_tab = self.ui.profile_tab;
                egui::Sides::new().show(
                    ui,
                    |ui| {
                        for (tab, title) in [
                            (ProfileTab::Outbounds, "节点"),
                            (ProfileTab::Routing, "分流规则"),
                            (ProfileTab::PortForward, pf_title.as_str()),
                            (ProfileTab::Advanced, "更多与源码"),
                        ] {
                            if crate::widgets::pill_tab(ui, title, next_tab == tab).clicked() {
                                next_tab = tab;
                            }
                        }
                    },
                    |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add_enabled_ui(enabled, |ui| {
                                self.proxy_draft_controls_inline(ui);
                            });
                        });
                    },
                );
                self.ui.profile_tab = next_tab;
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (tab, title) in [
                        (ProfileTab::Outbounds, "节点"),
                        (ProfileTab::Routing, "分流规则"),
                        (ProfileTab::PortForward, pf_title.as_str()),
                        (ProfileTab::Advanced, "更多与源码"),
                    ] {
                        if crate::widgets::pill_tab(ui, title, self.ui.profile_tab == tab).clicked()
                        {
                            self.ui.profile_tab = tab;
                        }
                    }
                });
                ui.add_enabled_ui(enabled, |ui| {
                    self.proxy_draft_controls(ui);
                });
            }
            ui.separator();
            // A small window may need page scrolling, but the editor retains
            // a usable minimum. Larger parents give all remaining height to it.
            let mut remaining = ui.available_rect_before_wrap();
            remaining.max.y = remaining.max.y.max(remaining.min.y + 300.0);
            let footer_id = ui.make_persistent_id(("profile-footer-height", &self.profile_id));
            let footer_height = ui
                .ctx()
                .data(|d| d.get_temp::<f32>(footer_id))
                .unwrap_or(68.0)
                .min((remaining.height() - 120.0).max(0.0));
            let sep_height = ui.spacing().item_spacing.y * 2.0 + 2.0;
            let body_height = (remaining.height() - footer_height - sep_height).max(80.0);
            ui.allocate_ui_with_layout(
                egui::vec2(remaining.width(), body_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    if self.ui.profile_tab == ProfileTab::Advanced
                        && self.ui.advanced_tab == AdvancedTab::Json
                    {
                        self.profile_body(ui, enabled && self.selected_profile == self.profile_id);
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("profile-body")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.profile_body(
                                    ui,
                                    enabled && self.selected_profile == self.profile_id,
                                )
                            });
                    }
                },
            );
            ui.separator();
            let footer_resp = ui.scope(|ui| {
                self.profile_footer(ui, enabled);
            });
            let actual_footer_height = footer_resp.response.rect.height();
            if (actual_footer_height - footer_height).abs() > 0.5 {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(footer_id, actual_footer_height));
                ui.ctx().request_repaint();
            }
        });
    }
    fn profile_footer(&mut self, ui: &mut egui::Ui, enabled: bool) {
        let pac_draft = self.pac_draft();
        if self.mode == ProxyMode::Pac
            && pac_draft.mode == xray_model::PacMode::Custom
            && pac_draft.script.trim().is_empty()
        {
            crate::widgets::banner_box(
                ui,
                crate::widgets::BadgeTone::Warn,
                "自定义 PAC 脚本为空；请在 PAC 页填写或导入脚本后再应用。",
            );
            ui.add_space(4.0);
        }
        let current = self
            .committed_id()
            .map(|id| self.ui.workspace.name(id))
            .unwrap_or("无");
        let status_hint = if self.has_draft_edits() || self.has_unapplied_runtime_diff() {
            format!(
                "待点击「检查并应用」生效到 Core · 当前{}：{current}",
                if self.is_running() {
                    "运行"
                } else {
                    "上次应用"
                }
            )
        } else {
            format!(
                "当前编辑内容与正在运行的 Core 一致 · 当前{}：{current}",
                if self.is_running() {
                    "运行"
                } else {
                    "上次应用"
                }
            )
        };
        let hint_tone = if self.has_draft_edits() || self.has_unapplied_runtime_diff() {
            crate::widgets::BadgeTone::Warn
        } else {
            crate::widgets::BadgeTone::Muted
        };
        let (hint_fg, _, _) = crate::widgets::tone_palette(ui, hint_tone);
        ui.add(
            egui::Label::new(RichText::new(status_hint).small().color(
                if hint_tone == crate::widgets::BadgeTone::Warn {
                    hint_fg
                } else {
                    ui.visuals().weak_text_color()
                },
            ))
            .truncate(),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    enabled && !self.ui.undo.is_empty(),
                    egui::Button::new("撤销"),
                )
                .clicked()
            {
                self.undo_edit();
            }
            if ui
                .add_enabled(
                    enabled && !self.ui.redo.is_empty(),
                    egui::Button::new("重做"),
                )
                .clicked()
            {
                self.redo_edit();
            }
            if self.has_draft_edits()
                && ui
                    .add_enabled(enabled, egui::Button::new("放弃修改"))
                    .clicked()
            {
                self.ui.dialog = Some(Dialog::Confirm(Action::Discard(self.profile_id.clone())));
            }
            if self.has_unapplied_runtime_diff()
                && ui
                    .add_enabled(enabled, egui::Button::new("还原运行版"))
                    .clicked()
            {
                self.revert_to_running_config();
            }
            let can_validate = enabled
                && !self.ui.draft_conflict
                && !self.core_path.is_empty()
                && self.selected_profile == self.profile_id;
            if ui
                .add_enabled(can_validate, egui::Button::new("预览变更"))
                .clicked()
            {
                self.validate_draft();
            }
            if self.candidate_matches() && ui.button("查看已检查的预览").clicked() {
                self.ui.preview_open = true;
            }
            if self.core_path.is_empty() && ui.button("先准备 Core").clicked() {
                self.ui.page = Page::Settings;
                self.ui.settings_tab = SettingsTab::Core;
            }
            if primary(ui, "检查并应用", can_validate) {
                self.validate_and_apply_draft();
            }
        });
    }
    fn jump_to_path(&mut self, path: String) {
        if path.starts_with("/outbounds") {
            self.ui.profile_tab = ProfileTab::Outbounds;
        } else if path.starts_with("/routing") {
            self.ui.profile_tab = ProfileTab::Routing;
        } else if path.starts_with("/inbounds") {
            let is_pf = path
                .strip_prefix("/inbounds/")
                .and_then(|rest| rest.split('/').next())
                .and_then(|idx| idx.parse::<usize>().ok())
                .and_then(|idx| {
                    serde_json::from_str::<serde_json::Value>(&self.editor)
                        .ok()
                        .and_then(|v| {
                            v.get("inbounds")?.as_array()?.get(idx).map(|item| {
                                let api = v.pointer("/api/tag").and_then(|t| t.as_str());
                                let tag = item.get("tag").and_then(|t| t.as_str());
                                let proto = item.get("protocol").and_then(|p| p.as_str());
                                proto == Some("dokodemo-door") && api != tag
                            })
                        })
                })
                .unwrap_or(false);
            if is_pf {
                self.ui.profile_tab = ProfileTab::PortForward;
            } else {
                self.ui.profile_tab = ProfileTab::Advanced;
                self.ui.advanced_tab = AdvancedTab::Inbounds;
                self.ui.inbounds_native = true;
            }
        } else {
            self.ui.profile_tab = ProfileTab::Advanced;
            self.ui.advanced_tab = AdvancedTab::Json;
        }
        self.ui.guided.native = false;
        self.ui.guided.notice = Some(format!("待补充字段：{path}"));
        self.ui.guided.focus = Some(path);
    }
    pub(in crate::app) fn profile_body(&mut self, ui: &mut egui::Ui, enabled: bool) {
        if let Some(path) = self.ui.number_inputs.keys().next().cloned() {
            ui.add(
                egui::Label::new(format!(
                    "有 {} 项未完成的数值输入，已暂存本地缓冲。补全后即可自动保存并检查应用。",
                    self.ui.number_inputs.len()
                ))
                .wrap(),
            );
            if ui.button("定位未完成数值").clicked() {
                if path.starts_with("/inbounds/") {
                    self.ui.profile_tab = ProfileTab::Advanced;
                    self.ui.advanced_tab = AdvancedTab::Inbounds;
                    self.ui.inbounds_native = true;
                } else if path.starts_with("/outbounds/") {
                    self.ui.profile_tab = ProfileTab::Outbounds;
                } else {
                    self.ui.profile_tab = ProfileTab::Routing;
                }
                self.ui.guided.native = true;
                self.ui.guided.focus = Some(path);
            }
        }
        if self.ui.profile_tab != ProfileTab::Advanced {
            match xray_config::parse_document(self.editor.as_bytes()) {
                Ok(value) => {
                    let show_review = self.ui.profile_tab == ProfileTab::Outbounds
                        || value
                            .pointer("/routing/rules")
                            .and_then(|v| v.as_array())
                            .is_none_or(Vec::is_empty);
                    if show_review
                        && !xray_config::inspect_config(&value).is_empty()
                        && let Some(path) = config_review_action(ui, &value)
                    {
                        self.jump_to_path(path);
                    }
                }
                Err(_) => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "JSON 语法无法解析，请在「更多与源码 → 原始 JSON」中修正。",
                    );
                }
            }
        }
        match self.ui.profile_tab {
            ProfileTab::Outbounds => {
                ui.horizontal_wrapped(|ui| {
                    let label = if self.ui.node_share_open {
                        "收起分享链接导入"
                    } else {
                        "从分享链接导入节点…"
                    };
                    if ui
                        .add_enabled(
                            enabled && self.ui.number_inputs.is_empty(),
                            egui::Button::new(label),
                        )
                        .clicked()
                    {
                        self.ui.node_share_open = !self.ui.node_share_open;
                        self.ui.node_share_feedback = None;
                    }
                });
                if enabled && self.ui.number_inputs.is_empty() && self.ui.node_share_open {
                    card(ui, |ui| {
                        ui.label("粘贴单条或多行分享链接（vless://、trojan://、ss://、vmess://、socks://、http://）");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.ui.node_share_input)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY)
                                .hint_text("每行一条分享链接"),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    !self.ui.node_share_input.trim().is_empty(),
                                    egui::Button::new("导入节点"),
                                )
                                .clicked()
                            {
                                self.import_share_links_into_editor();
                            }
                            if ui.button("清空").clicked() {
                                self.ui.node_share_input.clear();
                                self.ui.node_share_feedback = None;
                            }
                        });
                        if let Some(feedback) = &self.ui.node_share_feedback {
                            ui.colored_label(ui.visuals().warn_fg_color, feedback);
                        }
                    });
                    ui.add_space(6.0);
                }
                self.guided_or_native_section(ui, enabled, "outbounds", true);
                ui.label(
                    RichText::new("第一项为未命中路由规则时的默认出站，可通过「设为默认」切换。")
                        .small()
                        .weak(),
                );
            }
            ProfileTab::Routing => {
                self.guided_or_native_section(ui, enabled, "rules", false);
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&self.editor) {
                    let conflicts = xray_config::conflicts::conflicts(&value);
                    if !conflicts.is_empty() {
                        ui.separator();
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("发现 {} 项静态顺序/遮蔽风险", conflicts.len()),
                        );
                        for conflict in conflicts {
                            ui.label(
                                RichText::new(format!(
                                    "规则 {}：{}",
                                    conflict.index, conflict.message
                                ))
                                .small(),
                            );
                        }
                    }
                }
                ui.label(
                    RichText::new("按原生顺序匹配；静态预测不代表实际流量已通过该出站。")
                        .small()
                        .weak(),
                );
            }
            ProfileTab::PortForward => {
                self.port_forward_tab(ui, enabled && self.ui.number_inputs.is_empty());
            }
            ProfileTab::Advanced => {
                ui.horizontal_wrapped(|ui| {
                    let history_title = if self.history.is_empty() {
                        "历史版本".into()
                    } else {
                        format!("历史版本 ({})", self.history.len())
                    };
                    for (sub, title) in [
                        (AdvancedTab::Inbounds, "本地入口"),
                        (AdvancedTab::Json, "原始 JSON"),
                        (AdvancedTab::History, history_title.as_str()),
                    ] {
                        if crate::widgets::pill_tab(ui, title, self.ui.advanced_tab == sub)
                            .clicked()
                        {
                            self.ui.advanced_tab = sub;
                        }
                    }
                });
                ui.separator();
                match self.ui.advanced_tab {
                    AdvancedTab::Inbounds => {
                        ui.horizontal_wrapped(|ui| {
                            if crate::widgets::seg_button(
                                ui,
                                "常用卡片",
                                !self.ui.inbounds_native,
                                true,
                            )
                            .clicked()
                            {
                                self.ui.inbounds_native = false;
                            }
                            if crate::widgets::seg_button(
                                ui,
                                "高级：原生字段编辑",
                                self.ui.inbounds_native,
                                true,
                            )
                            .clicked()
                            {
                                self.ui.inbounds_native = true;
                            }
                        });
                        ui.add_space(6.0);
                        if self.ui.inbounds_native {
                            if let Ok(mut value) = serde_json::from_str(&self.editor) {
                                if !crate::port_forward::dokodemo_tags(&value).is_empty() {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(
                                                "提示：本配置包含 dokodemo-door 端口转发入站，可在「端口转发」页签用表单管理。",
                                            )
                                            .small()
                                            .weak(),
                                        );
                                        if ui.small_button("前往端口转发").clicked() {
                                            self.ui.profile_tab = ProfileTab::PortForward;
                                        }
                                    });
                                }
                                let pending_focus = self.ui.guided.focus.clone();
                                let changed = ui
                                    .push_id(
                                        (&self.profile_id, self.view_epoch, "inbounds"),
                                        |ui| {
                                            ui.add_enabled_ui(enabled, |ui| {
                                                let ch = crate::native_editor::edit_focused(
                                                    ui,
                                                    &mut value,
                                                    "inbounds",
                                                    pending_focus.as_deref(),
                                                    &mut self.ui.number_inputs,
                                                );
                                                self.ui.guided.focus = None;
                                                ch
                                            })
                                            .inner
                                        },
                                    )
                                    .inner;
                                if changed {
                                    self.ui.guided.notice = None;
                                    self.editor = serde_json::to_string_pretty(&value).unwrap();
                                    self.draft_edited();
                                }
                            } else {
                                ui.label("请先在原始 JSON 中修正语法。");
                            }
                        } else if let Ok(mut value) =
                            serde_json::from_str::<serde_json::Value>(&self.editor)
                        {
                            self.guided_inbounds_cards(
                                ui,
                                enabled && self.ui.number_inputs.is_empty(),
                                &mut value,
                            );
                        } else {
                            ui.label("请先在原始 JSON 中修正语法。");
                        }
                        ui.separator();
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Core 日志输出级别（log.loglevel）");
                            self.core_log_level_picker(
                                ui,
                                enabled && self.ui.number_inputs.is_empty(),
                                "profile-core-loglevel",
                            );
                        });
                        if ui
                            .checkbox(
                                &mut self.ui.allow_dynamic,
                                "允许动态外部依赖路径（不保证回滚内容）",
                            )
                            .on_hover_text("默认要求相对资源路径并冻结副本；显式允许后，应用与启动前会重新核对观察哈希，自动连接与自动恢复被禁用")
                            .changed()
                        {
                            self.invalidate_candidate();
                        }
                        ui.label(
                            RichText::new(
                                "系统代理接管前，需要验证兼容的本地 HTTP/Mixed 入口（回环或 0.0.0.0）。",
                            )
                            .small()
                            .weak(),
                        );
                    }
                    AdvancedTab::Json => {
                        if ui
                            .add_enabled(
                                enabled && self.ui.number_inputs.is_empty(),
                                egui::Button::new("格式化 JSON"),
                            )
                            .clicked()
                        {
                            match serde_json::from_str::<serde_json::Value>(&self.editor) {
                                Ok(value) => {
                                    self.editor = serde_json::to_string_pretty(&value).unwrap();
                                    self.draft_edited();
                                }
                                Err(error) => self.fail(error.into(), String::new()),
                            }
                        }
                        let paused_forwards = self
                            .ui
                            .workspace
                            .paused_port_forwards
                            .get(&self.profile_id)
                            .filter(|list| !list.is_empty())
                            .cloned();
                        let paused_reserve = if paused_forwards.is_some() { 90.0 } else { 0.0 };
                        let response = ui
                            .add_enabled_ui(enabled && self.ui.number_inputs.is_empty(), |ui| {
                                let note_height = ui.text_style_height(&egui::TextStyle::Small)
                                    + ui.spacing().item_spacing.y;
                                let height = (ui.available_height() - note_height - paused_reserve)
                                    .max(80.0);
                                json_editor(ui, &mut self.editor, "profile-json", height)
                            })
                            .inner;
                        if response.changed() {
                            self.draft_edited();
                        }
                        if let Some(paused_list) = paused_forwards {
                            let mut paused_json =
                                serde_json::to_string_pretty(&paused_list).unwrap_or_default();
                            egui::CollapsingHeader::new(format!(
                                "工作区停用端口转发暂存（{} 条 · 不写入 Core JSON）",
                                paused_list.len()
                            ))
                            .id_salt(("paused-pf-json", &self.profile_id))
                            .show(ui, |ui| {
                                json_viewer(ui, &mut paused_json, "paused-pf-viewer", 140.0);
                            });
                        }
                        ui.label(
                            RichText::new("原生 JSON · 未识别字段保留 · 应用前需真实 Core 校验")
                                .small()
                                .weak(),
                        );
                    }
                    AdvancedTab::History => {
                        ui.label(
                            RichText::new(
                                "历史版本按提交顺序展示。先比较差异，再决定是否载入；载入后仍需检查并应用。",
                            )
                            .small()
                            .weak(),
                        );
                        if self.history.is_empty() {
                            ui.label("暂无已提交的历史版本。");
                        }
                        for g in self.history.clone() {
                            let current = self.committed_id() == Some(g.id.as_str());
                            ui.horizontal_wrapped(|ui| {
                                ui.monospace(format!(
                                    "{} · {} · {}{}{}",
                                    g.id.chars().take(8).collect::<String>(),
                                    g.candidate.core.version,
                                    mode_name(g.proxy_mode),
                                    g.pac
                                        .as_ref()
                                        .map(|p| format!(
                                            " · PAC {}",
                                            p.revision.chars().take(8).collect::<String>()
                                        ))
                                        .unwrap_or_default(),
                                    if current { " · 当前运行" } else { "" }
                                ));
                                if ui
                                    .add_enabled(enabled, egui::Button::new("比较并载入…"))
                                    .clicked()
                                {
                                    if !self.ui.number_inputs.is_empty() {
                                        self.message =
                                            "请先补全未完成的数值输入，或明确放弃修改，再比较历史。"
                                                .into();
                                    } else {
                                        self.message = "正在准备历史与当前配置的比较…".into();
                                        self.send(Command::StageHistory {
                                            generation_id: g.id.clone(),
                                            expected_revision: self.revision.clone(),
                                        });
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }
    }
    fn import_share_links_into_editor(&mut self) {
        let batch = xray_config::share_link::parse_share_links(&self.ui.node_share_input);
        if batch.nodes.is_empty() {
            self.ui.node_share_feedback = Some(
                batch
                    .errors
                    .first()
                    .map(|e| format!("第 {} 行：{}", e.line, e.message))
                    .unwrap_or_else(|| "未识别到有效的分享链接。".into()),
            );
            return;
        }
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&self.editor) else {
            self.ui.node_share_feedback =
                Some("当前配置 JSON 语法有误，请先在原始 JSON 中修正。".into());
            return;
        };
        let mut names = self
            .ui
            .workspace
            .node_names
            .get(&self.profile_id)
            .cloned()
            .unwrap_or_default();
        match crate::guided_config::append_share_nodes(&mut value, &mut names, batch.nodes, None) {
            Ok((count, warnings)) => {
                self.ui
                    .workspace
                    .node_names
                    .insert(self.profile_id.clone(), names);
                self.save_workspace();
                self.editor = serde_json::to_string_pretty(&value).unwrap();
                self.draft_edited();
                self.ui.node_share_input.clear();
                let mut parts = vec![format!("已导入 {count} 个节点。")];
                if !batch.errors.is_empty() {
                    parts.push(format!(
                        "另有 {} 行解析失败（如第 {} 行：{}）。",
                        batch.errors.len(),
                        batch.errors[0].line,
                        batch.errors[0].message
                    ));
                }
                parts.extend(warnings);
                self.ui.node_share_feedback = Some(parts.join(" "));
                self.message = format!("已从分享链接导入 {count} 个节点。");
            }
            Err(err) => {
                self.ui.node_share_feedback = Some(err.into());
            }
        }
    }
    fn guided_or_native_section(
        &mut self,
        ui: &mut egui::Ui,
        enabled: bool,
        section: &str,
        is_outbounds: bool,
    ) {
        if let Ok(mut value) = serde_json::from_str(&self.editor) {
            let mut names = self
                .ui
                .workspace
                .node_names
                .get(&self.profile_id)
                .cloned()
                .unwrap_or_default();
            if let Some(notice) = &self.ui.guided.notice {
                ui.add(
                    egui::Label::new(RichText::new(notice).color(ui.visuals().warn_fg_color))
                        .wrap(),
                );
            }
            let pending_focus = self.ui.guided.focus.clone();
            let mut changed = false;
            let mut names_changed = false;
            ui.push_id((&self.profile_id, self.view_epoch, section), |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut self.ui.guided.native, false, "常用表单");
                        ui.selectable_value(&mut self.ui.guided.native, true, "高级：原生字段编辑");
                    });
                    if self.ui.guided.native {
                        changed |= crate::native_editor::edit_focused(
                            ui,
                            &mut value,
                            section,
                            pending_focus.as_deref(),
                            &mut self.ui.number_inputs,
                        );
                        self.ui.guided.focus = None;
                    } else if self.ui.number_inputs.is_empty() {
                        self.ui.guided.pf_refs = crate::port_forward::outbound_ref_counts(
                            &value,
                            &self.ui.workspace,
                            &self.profile_id,
                        );
                        self.ui.guided.pf_names = self
                            .ui
                            .workspace
                            .port_forward_names
                            .get(&self.profile_id)
                            .cloned()
                            .unwrap_or_default();
                        let result = if is_outbounds {
                            crate::guided_config::edit_nodes(
                                ui,
                                &mut value,
                                &mut names,
                                &mut self.ui.guided,
                            )
                        } else {
                            crate::guided_config::edit_routes(
                                ui,
                                &mut value,
                                &names,
                                &mut self.ui.guided,
                            )
                        };
                        changed |= result.changed;
                        names_changed = result.names_changed;
                        if result.port_forward {
                            self.ui.profile_tab = ProfileTab::PortForward;
                        }
                        if result.raw {
                            self.ui.guided.native = true;
                            self.ui.guided.focus = pending_focus;
                        }
                    }
                });
            });
            if names_changed {
                self.ui
                    .workspace
                    .node_names
                    .insert(self.profile_id.clone(), names);
                self.save_workspace();
            }
            if changed {
                self.ui.guided.notice = None;
                self.editor = serde_json::to_string_pretty(&value).unwrap();
                self.draft_edited();
            }
        } else {
            ui.label("请先在「更多与源码 → 原始 JSON」中修正语法。");
        }
    }
    fn port_forward_tab(&mut self, ui: &mut egui::Ui, enabled: bool) {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&self.editor) else {
            ui.label("请先在「更多与源码 → 原始 JSON」中修正语法。");
            return;
        };

        let entries =
            crate::port_forward::extract_entries(&value, &self.ui.workspace, &self.profile_id);
        let paused = self
            .ui
            .workspace
            .paused_port_forwards
            .get(&self.profile_id)
            .cloned()
            .unwrap_or_default();
        let node_names = self
            .ui
            .workspace
            .node_names
            .get(&self.profile_id)
            .cloned()
            .unwrap_or_default();
        let available_outbounds = crate::port_forward::available_outbounds(&value, &node_names);
        let inbound_conflicts: std::collections::HashMap<String, String> = value
            .get("inbounds")
            .and_then(serde_json::Value::as_array)
            .map(|inbounds| {
                xray_config::inbound_port_conflicts(inbounds)
                    .into_iter()
                    .filter_map(|(idx, msg)| {
                        let tag = inbounds.get(idx)?.get("tag")?.as_str()?.to_owned();
                        Some((tag, msg))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let runtime_entries = self
            .committed_id()
            .filter(|id| *id == self.profile_id && self.is_running())
            .and_then(|_| {
                let gen_id = self.status.as_ref()?.generation.as_ref()?.id.as_str();
                let rt = self
                    .ui
                    .runtime
                    .as_ref()
                    .filter(|r| r.generation_id == gen_id)?;
                let rt_json: serde_json::Value = serde_json::from_str(&rt.json).ok()?;
                Some(crate::port_forward::extract_entries(
                    &rt_json,
                    &self.ui.workspace,
                    &self.profile_id,
                ))
            })
            .unwrap_or_default();

        let active_count = entries.iter().filter(|e| e.enabled).count();
        crate::widgets::subcard(ui, None, true, |ui| {
            if ui.available_width() >= 560.0 {
                egui::Sides::new().show(
                    ui,
                    |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("本地端口转发（dokodemo-door）")
                                        .strong()
                                        .size(15.5),
                                );
                                crate::widgets::badge(
                                    ui,
                                    &format!(
                                        "共 {} 条 · 已启用 {} 条 · 已停用 {} 条",
                                        entries.len(),
                                        active_count,
                                        entries.len().saturating_sub(active_count)
                                    ),
                                    crate::widgets::BadgeTone::Muted,
                                );
                            });
                            ui.label(
                                RichText::new(
                                    "不依赖系统代理模式；启用条目自动写入 inbounds[] 并在 routing.rules[] 置顶，停用条目保留参数于工作区。",
                                )
                                .small()
                                .weak(),
                            );
                        });
                    },
                    |ui| {
                        if primary(ui, "+ 添加端口转发", enabled) {
                            self.ui.pf_form =
                                crate::port_forward::PortForwardForm::from_new(&value, &paused);
                            self.ui.pf_error = None;
                            self.ui.dialog = Some(Dialog::PortForward);
                        }
                    },
                );
            } else {
                ui.label(
                    RichText::new("本地端口转发（dokodemo-door）")
                        .strong()
                        .size(15.5),
                );
                ui.label(
                    RichText::new(
                        "不依赖系统代理模式；启用条目自动写入 inbounds[] 并在 routing.rules[] 置顶，停用条目保留参数于工作区。",
                    )
                    .small()
                    .weak(),
                );
                ui.horizontal_wrapped(|ui| {
                    if primary(ui, "+ 添加端口转发", enabled) {
                        self.ui.pf_form =
                            crate::port_forward::PortForwardForm::from_new(&value, &paused);
                        self.ui.pf_error = None;
                        self.ui.dialog = Some(Dialog::PortForward);
                    }
                    crate::widgets::badge(
                        ui,
                        &format!(
                            "共 {} 条 · 已启用 {} 条 · 已停用 {} 条",
                            entries.len(),
                            active_count,
                            entries.len().saturating_sub(active_count)
                        ),
                        crate::widgets::BadgeTone::Muted,
                    );
                });
            }
        });
        ui.add_space(8.0);

        if entries.is_empty() {
            crate::widgets::subcard(ui, None, false, |ui| {
                ui.label(
                    RichText::new("暂无端口转发规则。点击右上角「+ 添加端口转发」新增。").weak(),
                );
            });
            return;
        }

        let mut json_changed = false;
        let mut ws_changed = false;
        for entry in entries {
            ui.push_id((&entry.tag, entry.paused_index), |ui| {
                let conflict_msg = if entry.enabled {
                    inbound_conflicts.get(&entry.tag).cloned()
                } else {
                    None
                };
                let missing_outbound = !available_outbounds
                    .iter()
                    .any(|(tag, _)| *tag == entry.outbound_tag);
                let border_tone = if conflict_msg.is_some() || missing_outbound {
                    Some(crate::widgets::BadgeTone::Danger)
                } else {
                    None
                };
                crate::widgets::subcard(ui, border_tone, !entry.enabled, |ui| {
                    let expose_lan = !crate::port_forward::is_loopback_listen(&entry.listen);
                    let outbound_is_direct = value
                        .get("outbounds")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|arr| {
                            arr.iter().any(|o| {
                                o.get("tag").and_then(|t| t.as_str()) == Some(&entry.outbound_tag)
                                    && o.get("protocol").and_then(|p| p.as_str()) == Some("freedom")
                            })
                        });
                    let private_target_via_proxy = !outbound_is_direct
                        && crate::port_forward::is_private_or_loopback_host(&entry.target_host);

                    ui.horizontal_wrapped(|ui| {
                        let mut active = entry.enabled;
                        if ui
                            .add_enabled(enabled, egui::Checkbox::new(&mut active, ""))
                            .on_hover_text("取消勾选可保留参数但暂停该端口监听")
                            .changed()
                        {
                            match crate::port_forward::set_entry_enabled(
                                &mut value,
                                &mut self.ui.workspace,
                                &self.profile_id,
                                &entry,
                                active,
                            ) {
                                Ok(()) => {
                                    if active {
                                        json_changed = true;
                                        ws_changed = true;
                                        self.message = format!(
                                            "已启用端口转发：{}（请点击底栏「检查并应用」生效）",
                                            entry.tag
                                        );
                                    } else {
                                        json_changed = true;
                                        ws_changed = true;
                                        self.message = format!(
                                            "已停用端口转发：{}（参数已保留于工作区）",
                                            entry.tag
                                        );
                                    }
                                }
                                Err(err) => self.message = err,
                            }
                        }
                        let title = if entry.name.is_empty() {
                            format!("转发 {} -> {}", entry.port, entry.target_port)
                        } else {
                            entry.name.clone()
                        };
                        ui.label(RichText::new(title).strong().size(15.0));
                        crate::widgets::badge(
                            ui,
                            crate::port_forward::network_label(&entry.network),
                            crate::widgets::BadgeTone::Accent,
                        );
                        crate::widgets::mono_badge(
                            ui,
                            &entry.tag,
                            crate::widgets::BadgeTone::Muted,
                        );

                        let is_running_match = entry.enabled
                            && runtime_entries.iter().any(|r| {
                                r.enabled
                                    && r.tag == entry.tag
                                    && r.listen == entry.listen
                                    && r.port == entry.port
                                    && r.network == entry.network
                                    && r.target_host == entry.target_host
                                    && r.target_port == entry.target_port
                                    && r.outbound_tag == entry.outbound_tag
                            });
                        if !entry.enabled {
                            crate::widgets::badge(
                                ui,
                                "已停用（参数保留）",
                                crate::widgets::BadgeTone::Muted,
                            );
                        } else if conflict_msg.is_some() || missing_outbound {
                            crate::widgets::badge(
                                ui,
                                "配置异常",
                                crate::widgets::BadgeTone::Danger,
                            );
                        } else if is_running_match {
                            crate::widgets::badge(
                                ui,
                                "生效中",
                                crate::widgets::BadgeTone::Success,
                            );
                        } else {
                            crate::widgets::badge(
                                ui,
                                "待应用",
                                crate::widgets::BadgeTone::Warn,
                            );
                        }
                        if expose_lan {
                            crate::widgets::badge(
                                ui,
                                "局域网暴露 · 无认证",
                                crate::widgets::BadgeTone::Warn,
                            );
                        }
                        ui.add_space(6.0);
                        let copy_ip = if entry.listen == crate::port_forward::LISTEN_LAN {
                            crate::port_forward::LISTEN_LOCAL
                        } else {
                            entry.listen.as_str()
                        };
                        let copy_addr = format!("{copy_ip}:{}", entry.port);
                        if ui.small_button("复制监听地址").clicked() {
                            ui.ctx().copy_text(copy_addr.clone());
                            self.message = format!("已复制监听地址：{copy_addr}");
                        }
                        if ui
                            .add_enabled(enabled, egui::Button::new("编辑").small())
                            .clicked()
                        {
                            self.ui.pf_form =
                                crate::port_forward::PortForwardForm::from_entry(&entry);
                            self.ui.pf_error = None;
                            self.ui.dialog = Some(Dialog::PortForward);
                        }
                        if ui
                            .add_enabled(enabled, egui::Button::new("删除").small())
                            .clicked()
                        {
                            crate::port_forward::remove_entry(
                                &mut value,
                                &mut self.ui.workspace,
                                &self.profile_id,
                                &entry,
                            );
                            if entry.enabled {
                                json_changed = true;
                            }
                            ws_changed = true;
                            self.message = format!("已移除端口转发：{}", entry.tag);
                        }
                    });

                    let outbound_display = available_outbounds
                        .iter()
                        .find(|(tag, _)| *tag == entry.outbound_tag)
                        .map(|(_, label)| label.as_str())
                        .unwrap_or(&entry.outbound_tag);
                    let target_text = if entry.target_host.is_empty() {
                        "未填写目标".to_owned()
                    } else if entry.target_port > 0 {
                        format!("{}:{}", entry.target_host, entry.target_port)
                    } else {
                        entry.target_host.clone()
                    };
                    let outbound_tone = if missing_outbound {
                        crate::widgets::BadgeTone::Danger
                    } else if outbound_is_direct {
                        crate::widgets::BadgeTone::Success
                    } else {
                        crate::widgets::BadgeTone::Accent
                    };
                    crate::widgets::flow_strip(ui, |ui| {
                        ui.monospace(format!("{}:{}", entry.listen, entry.port));
                        ui.label(RichText::new("-> 经由").small().weak());
                        crate::widgets::badge(ui, outbound_display, outbound_tone);
                        ui.label(RichText::new("-> 目标").small().weak());
                        ui.monospace(&target_text);
                    });

                    if let Some(msg) = conflict_msg {
                        crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Danger, &msg);
                    }
                    if missing_outbound {
                        crate::widgets::banner_box(
                            ui,
                            crate::widgets::BadgeTone::Danger,
                            &format!(
                                "所选出站「{}」在「节点」页已被移除，请编辑重新指定出口。",
                                entry.outbound_tag
                            ),
                        );
                    } else if entry.missing_route {
                        crate::widgets::banner_box(
                            ui,
                            crate::widgets::BadgeTone::Warn,
                            "未配置专属路由规则，当前将走默认访问出站；点「编辑」并保存可自动生成置顶规则。",
                        );
                    } else if entry.custom_route {
                        crate::widgets::banner_box(
                            ui,
                            crate::widgets::BadgeTone::Muted,
                            "此入站包含复合原生路由规则；保存表单不会覆盖原有复合路由。",
                        );
                    }
                    if expose_lan {
                        crate::widgets::banner_box(
                            ui,
                            crate::widgets::BadgeTone::Warn,
                            "dokodemo-door 无密码认证保护，同局域网设备可直接连接此端口。",
                        );
                    }
                    if private_target_via_proxy {
                        crate::widgets::banner_box(
                            ui,
                            crate::widgets::BadgeTone::Accent,
                            "提示：目标为内网/回环地址，请确认远端服务端路由未通过 geoip:private 拦截。",
                        );
                    }
                });
            });
            ui.add_space(8.0);
        }

        if ws_changed {
            self.save_workspace();
        }
        if json_changed {
            self.editor = serde_json::to_string_pretty(&value).unwrap();
            self.draft_edited();
        }
    }
    fn guided_inbounds_cards(
        &mut self,
        ui: &mut egui::Ui,
        enabled: bool,
        value: &mut serde_json::Value,
    ) {
        let mut changed = false;
        let inbound_conflicts: std::collections::HashMap<usize, String> = value
            .get("inbounds")
            .and_then(serde_json::Value::as_array)
            .map(|inbounds| {
                xray_config::inbound_port_conflicts(inbounds)
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default();

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("系统代理 / 本地代理入口").strong());
            if ui
                .add_enabled(enabled, egui::Button::new("+ 添加 HTTP 入口").small())
                .clicked()
            {
                let paused = self
                    .ui
                    .workspace
                    .paused_port_forwards
                    .get(&self.profile_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let next_port = crate::port_forward::next_free_port(value, paused);
                let arr = value
                    .as_object_mut()
                    .expect("config object")
                    .entry("inbounds")
                    .or_insert_with(|| serde_json::json!([]))
                    .as_array_mut();
                if let Some(inbounds) = arr {
                    let tag = format!("http-{}", inbounds.len() + 1);
                    inbounds.push(serde_json::json!({
                        "tag": tag,
                        "listen": "127.0.0.1",
                        "port": next_port,
                        "protocol": "http",
                        "settings": {}
                    }));
                    changed = true;
                }
            }
            if ui
                .add_enabled(enabled, egui::Button::new("+ 添加 SOCKS 入口").small())
                .clicked()
            {
                let paused = self
                    .ui
                    .workspace
                    .paused_port_forwards
                    .get(&self.profile_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let next_port = crate::port_forward::next_free_port(value, paused);
                let arr = value
                    .as_object_mut()
                    .expect("config object")
                    .entry("inbounds")
                    .or_insert_with(|| serde_json::json!([]))
                    .as_array_mut();
                if let Some(inbounds) = arr {
                    let tag = format!("socks-{}", inbounds.len() + 1);
                    inbounds.push(serde_json::json!({
                        "tag": tag,
                        "listen": "127.0.0.1",
                        "port": next_port,
                        "protocol": "socks",
                        "settings": {"udp": true}
                    }));
                    changed = true;
                }
            }
        });
        ui.add_space(6.0);

        let api_tag = value
            .pointer("/api/tag")
            .and_then(|t| t.as_str())
            .map(str::to_owned);
        let mut remove_idx = None;
        let mut jump_port_forward = false;

        if let Some(inbounds) = value
            .get_mut("inbounds")
            .and_then(serde_json::Value::as_array_mut)
        {
            if inbounds.is_empty() {
                ui.label(RichText::new("当前配置暂无本地入口。").weak());
            }
            for (index, inbound) in inbounds.iter_mut().enumerate() {
                let tag = inbound
                    .get("tag")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未命名")
                    .to_owned();
                let proto = inbound
                    .get("protocol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_owned();
                let is_pf = proto == "dokodemo-door" && api_tag.as_deref() != Some(tag.as_str());
                let sniffing_on = inbound
                    .pointer("/sniffing/enabled")
                    .and_then(|v| v.as_bool())
                    == Some(true);

                ui.push_id(("guided-inbound", index, &tag), |ui| {
                    crate::widgets::subcard(
                        ui,
                        inbound_conflicts
                            .contains_key(&index)
                            .then_some(crate::widgets::BadgeTone::Danger),
                        is_pf,
                        |ui| {
                            if is_pf {
                                let listen = inbound
                                    .get("listen")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("0.0.0.0");
                                let port =
                                    inbound.get("port").and_then(|v| v.as_u64()).unwrap_or(0);
                                let t_host = inbound
                                    .pointer("/settings/address")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let t_port = inbound
                                    .pointer("/settings/port")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&tag).strong());
                                    crate::widgets::badge(
                                        ui,
                                        "dokodemo-door",
                                        crate::widgets::BadgeTone::Muted,
                                    );
                                    crate::widgets::badge(
                                        ui,
                                        "端口转发自动维护",
                                        crate::widgets::BadgeTone::Accent,
                                    );
                                    ui.monospace(format!("{listen}:{port} → {t_host}:{t_port}"));
                                    if ui.small_button("前往端口转发").clicked() {
                                        jump_port_forward = true;
                                    }
                                });
                                return;
                            }

                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(&tag).strong());
                                crate::widgets::badge(
                                    ui,
                                    &proto,
                                    crate::widgets::BadgeTone::Accent,
                                );
                                crate::widgets::badge(
                                    ui,
                                    if sniffing_on {
                                        "Sniffing: 已开启"
                                    } else {
                                        "Sniffing: 关闭"
                                    },
                                    if sniffing_on {
                                        crate::widgets::BadgeTone::Success
                                    } else {
                                        crate::widgets::BadgeTone::Muted
                                    },
                                );
                                if let Some(err) = inbound_conflicts.get(&index) {
                                    crate::widgets::badge(
                                        ui,
                                        err,
                                        crate::widgets::BadgeTone::Danger,
                                    );
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label("监听");
                                let mut listen_str = inbound
                                    .get("listen")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("127.0.0.1")
                                    .to_owned();
                                if ui
                                    .add_enabled(
                                        enabled,
                                        crate::widgets::singleline(&mut listen_str, 130.0),
                                    )
                                    .changed()
                                {
                                    inbound["listen"] = serde_json::json!(listen_str.trim());
                                    changed = true;
                                }
                                ui.label("端口");
                                let mut port_str = inbound
                                    .get("port")
                                    .and_then(|v| v.as_u64())
                                    .map(|p| p.to_string())
                                    .unwrap_or_default();
                                if ui
                                    .add_enabled(
                                        enabled,
                                        crate::widgets::singleline(&mut port_str, 90.0),
                                    )
                                    .changed()
                                    && let Ok(port_num) = port_str.trim().parse::<u16>()
                                    && port_num > 0
                                {
                                    inbound["port"] = serde_json::json!(port_num);
                                    changed = true;
                                }
                                if ui
                                    .add_enabled(enabled, egui::Button::new("删除").small())
                                    .clicked()
                                {
                                    remove_idx = Some(index);
                                }
                            });
                        },
                    );
                    ui.add_space(6.0);
                });
            }
            if let Some(idx) = remove_idx {
                inbounds.remove(idx);
                changed = true;
            }
        }

        if jump_port_forward {
            self.ui.profile_tab = ProfileTab::PortForward;
        }
        if changed {
            self.ui.guided.notice = None;
            self.editor = serde_json::to_string_pretty(value).unwrap();
            self.draft_edited();
        }
    }
    pub(in crate::app) fn proxy_draft_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| self.proxy_draft_controls_inline(ui));
    }
    fn proxy_draft_controls_inline(&mut self, ui: &mut egui::Ui) {
        let before = (self.mode, self.http_tag.clone());
        let listeners = self.compatible_http_listeners();
        if matches!(self.mode, ProxyMode::Global | ProxyMode::Pac)
            && !listeners.iter().any(|(tag, _)| tag == &self.http_tag)
            && let Some((first, _)) = listeners.first()
        {
            self.http_tag = first.clone();
        }
        let has_http = !listeners.is_empty();
        ui.label(RichText::new("代理方式：").small().weak());
        for (mode, short_label) in [
            (ProxyMode::Direct, "直连"),
            (ProxyMode::Pac, "PAC"),
            (ProxyMode::Global, "全局"),
        ] {
            let allowed = mode == ProxyMode::Direct || has_http;
            if crate::widgets::seg_button(ui, short_label, self.mode == mode, allowed).clicked() {
                self.mode = mode;
            }
        }
        if listeners.len() > 1 {
            let selected = listeners
                .iter()
                .find(|(tag, _)| tag == &self.http_tag)
                .map(|(_, label)| label.clone())
                .unwrap_or_else(|| "请选择本地 HTTP 入口".into());
            egui::ComboBox::from_id_salt("proxy-http-listener")
                .width(ui.available_width().min(190.0))
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (tag, label) in &listeners {
                        ui.selectable_value(&mut self.http_tag, tag.clone(), label);
                    }
                });
        } else if let Some((_, label)) = listeners.first() {
            crate::widgets::badge(
                ui,
                &format!("HTTP: {label}"),
                crate::widgets::BadgeTone::Muted,
            );
        } else {
            crate::widgets::badge(ui, "无 HTTP 入口", crate::widgets::BadgeTone::Warn);
        }
        if before.0 != self.mode {
            let profile_id = self.profile_id.clone();
            self.save_profile_proxy_mode(&profile_id, self.mode);
        }
        if before != (self.mode, self.http_tag.clone()) {
            self.invalidate_candidate();
        }
    }
}
