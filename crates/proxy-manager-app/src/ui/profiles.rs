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
        if ui.available_width() >= 840.0 {
            let picker_width = (ui.available_width() * 0.20).clamp(204.0, 256.0).floor();
            let height = (viewport_height - (ui.cursor().top() - page_top)).max(400.0);
            let list_max_height = (height - 196.0).clamp(150.0, 700.0);
            let editor_width = ui.available_width() - picker_width - ui.spacing().item_spacing.x;
            ui.horizontal_top(|ui| {
                let picker = ui.allocate_ui_with_layout(
                    egui::vec2(picker_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.profile_picker(ui, enabled, true, list_max_height),
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(editor_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.profile_editor(ui, enabled, picker.response.rect.height()),
                );
            });
        } else {
            self.profile_picker(ui, enabled, false, 0.0);
            ui.add_space(10.0);
            let height = (viewport_height - (ui.cursor().top() - page_top)).max(400.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.profile_editor(ui, enabled, 0.0),
            );
        }
    }
    fn profile_picker(
        &mut self,
        ui: &mut egui::Ui,
        enabled: bool,
        vertical: bool,
        list_max_height: f32,
    ) {
        card(ui, |ui| {
            if vertical {
                ui.label(
                    RichText::new(format!("我的配置 · {}", self.profiles.len()))
                        .small()
                        .weak(),
                );
                if !self.profiles.is_empty() {
                    egui::ScrollArea::vertical()
                        .max_height(list_max_height)
                        .id_salt("profile-list")
                        .show(ui, |ui| {
                            for profile in self.profiles.clone() {
                                self.profile_choice(ui, profile, true);
                            }
                        });
                }
                let action = profile_action_buttons(ui, enabled, ui.available_width(), false);
                self.apply_profile_picker_action(action);
            } else {
                let count_label = format!("我的配置 · {}", self.profiles.len());
                let mut action = None;
                crate::widgets::responsive_header(
                    ui,
                    420.0,
                    |ui| {
                        ui.label(RichText::new(&count_label).small().weak());
                    },
                    |ui, rtl| {
                        action = profile_action_buttons(ui, enabled, 80.0, rtl);
                    },
                );
                self.apply_profile_picker_action(action);
                if !self.profiles.is_empty() {
                    egui::ScrollArea::horizontal()
                        .id_salt("profile-strip")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for profile in self.profiles.clone() {
                                    self.profile_choice(ui, profile, false);
                                }
                            });
                        });
                }
            }
        });
    }
    fn apply_profile_picker_action(&mut self, action: Option<usize>) {
        match action {
            Some(0) => self.begin_create(false, false),
            Some(1) => self.begin_create(true, false),
            Some(2) => {
                self.refresh_profiles();
                if !self.profile_id.is_empty() {
                    self.send(Command::GetProfile {
                        profile_id: self.profile_id.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    fn profile_choice(&mut self, ui: &mut egui::Ui, p: ProfileSummary, vertical: bool) {
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
        let hover_text = format!("{title}\n{meta}");
        let card_width = if vertical {
            (ui.available_width() - 2.0).max(140.0)
        } else {
            188.0
        };
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
            .on_hover_text(hover_text)
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if click_resp.clicked() && self.selected_profile != p.id {
            if self.ui.draft_conflict || self.ui.unreadable_drafts.contains(&self.profile_id) {
                self.ui.dialog = Some(Dialog::Confirm(Action::Switch(p.id)));
            } else {
                self.switch_profile(p.id);
            }
        }
        if vertical {
            ui.add_space(4.0);
        }
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
            let (status_text, status_tone) = if self.has_draft_edits() {
                ("未落盘修改", crate::widgets::BadgeTone::Warn)
            } else if self.has_unapplied_runtime_diff() {
                ("已保存 · 待应用", crate::widgets::BadgeTone::Warn)
            } else if self.committed_id() == Some(self.profile_id.as_str()) && self.is_running() {
                ("运行中", crate::widgets::BadgeTone::Success)
            } else {
                ("已保存", crate::widgets::BadgeTone::Muted)
            };
            let profile_name = self.ui.workspace.name(&self.profile_id).to_owned();
            let mut rename_value = self.ui.inline_rename.clone();
            let renaming = rename_value.is_some();
            let mut start_rename = false;
            let mut enter_confirm = false;
            let mut btn_confirm = false;
            let mut cancel_rename = false;
            let mut duplicate_clicked = false;
            let mut delete_clicked = false;
            let mut share_clicked = false;
            let can_copy = enabled && self.ui.number_inputs.is_empty();
            let can_delete = enabled
                && self.selected_profile == self.profile_id
                && self.committed_id() != Some(self.profile_id.as_str());
            let can_share = self.ui.number_inputs.is_empty();

            crate::widgets::responsive_header(
                ui,
                560.0,
                |ui| {
                    ui.horizontal(|ui| {
                        if let Some(name) = rename_value.as_mut() {
                            let rename_w = (ui.available_width() - 128.0).clamp(120.0, 240.0);
                            let response = ui.add(singleline(name, rename_w).char_limit(80));
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                enter_confirm = true;
                            }
                        } else {
                            let max_title_w = (ui.available_width() - 128.0).max(72.0);
                            let title_resp = ui
                                .scope(|ui| {
                                    ui.set_max_width(max_title_w);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&profile_name).strong().size(20.0),
                                        )
                                        .truncate(),
                                    )
                                })
                                .inner;
                            title_resp.on_hover_text(&profile_name);
                        }
                        crate::widgets::badge(ui, status_text, status_tone);
                    });
                },
                |ui, rtl| {
                    if renaming {
                        for idx in if rtl { [1, 0] } else { [0, 1] } {
                            match idx {
                                0 if primary(ui, "确定", true) => btn_confirm = true,
                                1 if ui.button("取消").clicked() => cancel_rename = true,
                                _ => {}
                            }
                        }
                    } else {
                        for idx in if rtl { [3, 2, 1, 0] } else { [0, 1, 2, 3] } {
                            match idx {
                                0 if ui.button("改名").clicked() => start_rename = true,
                                1 if ui
                                    .add_enabled(can_copy, egui::Button::new("复制"))
                                    .on_disabled_hover_text("请先补全未完成的数值输入，再复制配置")
                                    .clicked() =>
                                {
                                    duplicate_clicked = true;
                                }
                                2 if ui
                                    .add_enabled(can_delete, egui::Button::new("删除"))
                                    .on_disabled_hover_text(
                                        "上次已应用的配置仍被运行记录引用，需先应用另一份配置",
                                    )
                                    .clicked() =>
                                {
                                    delete_clicked = true;
                                }
                                3 if ui
                                    .add_enabled(can_share, egui::Button::new("分享 / 导出"))
                                    .on_disabled_hover_text(
                                        "请先补全未完成的数值输入，再分享或导出配置；输入已随草稿保留",
                                    )
                                    .clicked() =>
                                {
                                    share_clicked = true;
                                }
                                _ => {}
                            }
                        }
                    }
                },
            );

            if enter_confirm || btn_confirm {
                if let Some(name) = rename_value {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        self.ui
                            .workspace
                            .names
                            .insert(self.profile_id.clone(), trimmed.chars().take(80).collect());
                        self.save_workspace();
                    }
                }
                self.ui.inline_rename = None;
            } else if cancel_rename {
                self.ui.inline_rename = None;
            } else if rename_value.is_some() {
                self.ui.inline_rename = rename_value;
            }
            if start_rename {
                self.ui.inline_rename = Some(profile_name);
            }
            if duplicate_clicked {
                self.begin_create(false, true);
            }
            if delete_clicked {
                self.ui.dialog = Some(Dialog::Confirm(Action::Delete {
                    profile: self.profile_id.clone(),
                    revision: self.revision.clone(),
                }));
            }
            if share_clicked {
                self.open_share_dialog(super::share::ShareTab::NodeLinks, None);
            }
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
            let pf_count = self.profile_port_forward_count(&self.profile_id)
                + self.ui.ssh_state.forwards.len();
            let pf_title = if pf_count > 0 {
                format!("端口转发 ({pf_count})")
            } else {
                "端口转发".into()
            };
            if ui.available_width() >= 750.0 {
                let mut next_tab = self.ui.profile_tab;
                let before = (self.mode, self.http_tag.clone());
                let mut next_mode = self.mode;
                let mut next_http = self.http_tag.clone();
                let listeners = self.compatible_http_listeners();
                if matches!(next_mode, ProxyMode::Global | ProxyMode::Pac)
                    && !listeners.iter().any(|(tag, _)| tag == &next_http)
                    && let Some((first, _)) = listeners.first()
                {
                    next_http = first.clone();
                }
                egui::Sides::new().shrink_left().wrap().show(
                    ui,
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
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
                        });
                    },
                    |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            render_proxy_draft_controls(
                                ui,
                                &listeners,
                                &mut next_mode,
                                &mut next_http,
                                true,
                            );
                        });
                    },
                );
                self.ui.profile_tab = next_tab;
                self.apply_proxy_draft_selection(before, next_mode, next_http);
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
                                ui.set_max_width(ui.available_width());
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
            && pac_draft.mode == proxy_model::PacMode::Custom
            && pac_draft.script.trim().is_empty()
        {
            crate::widgets::banner_box(
                ui,
                crate::widgets::BadgeTone::Warn,
                "自定义 PAC 脚本为空；请在 PAC 页填写或导入脚本后再应用。",
            );
            ui.add_space(4.0);
        }
        let current_full = self
            .committed_id()
            .map(|id| self.ui.workspace.name(id))
            .unwrap_or("无");
        let current_short = crate::widgets::ellipsize(current_full, 24);
        let state_label = if self.is_running() {
            "运行"
        } else {
            "上次应用"
        };
        let prefix = if self.has_draft_edits() || self.has_unapplied_runtime_diff() {
            "待点击「检查并应用」生效到 Core"
        } else {
            "当前编辑内容与正在运行的 Core 一致"
        };
        let status_hint = format!("{prefix} · 当前{state_label}：{current_short}");
        let full_status_hint = format!("{prefix} · 当前{state_label}：{current_full}");
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
            .wrap(),
        )
        .on_hover_text(full_status_hint);
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
            match proxy_config::parse_document(self.editor.as_bytes()) {
                Ok(value) => {
                    let show_review = self.ui.profile_tab == ProfileTab::Outbounds
                        || value
                            .pointer("/routing/rules")
                            .and_then(|v| v.as_array())
                            .is_none_or(Vec::is_empty);
                    if show_review
                        && !proxy_config::inspect_config(&value).is_empty()
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
                    let conflicts = proxy_config::conflicts::conflicts(&value);
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
                                        if ui.button("前往端口转发").clicked() {
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
        let batch = proxy_config::share_link::parse_share_links(&self.ui.node_share_input);
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
            let mut share_node = None;
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
                        share_node = result.share_node;
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
            if let Some(idx) = share_node {
                self.open_share_dialog(super::share::ShareTab::NodeLinks, Some(idx));
            }
        } else {
            ui.label("请先在「更多与源码 → 原始 JSON」中修正语法。");
        }
    }
    fn port_forward_tab(&mut self, ui: &mut egui::Ui, enabled: bool) {
        let host_count = self.ui.ssh_state.hosts.len();
        let cred_count = self.ui.ssh_state.credentials.len();
        let host_tab = if host_count > 0 {
            format!("SSH 跳板机 ({host_count})")
        } else {
            "SSH 跳板机".into()
        };
        let cred_tab = if cred_count > 0 {
            format!("SSH 凭证 ({cred_count})")
        } else {
            "SSH 凭证".into()
        };
        let sub_tabs = [
            (PortForwardSubTab::Forwards, "转发规则".to_owned()),
            (PortForwardSubTab::SshHosts, host_tab),
            (PortForwardSubTab::SshCredentials, cred_tab),
        ];
        let render_subtabs = |ui: &mut egui::Ui, current: &mut PortForwardSubTab| {
            ui.horizontal_wrapped(|ui| {
                for (sub, label) in &sub_tabs {
                    if crate::widgets::seg_button(ui, label, *current == *sub, true).clicked() {
                        *current = *sub;
                    }
                }
            });
        };
        if self.ui.pf_subtab != PortForwardSubTab::Forwards {
            render_subtabs(ui, &mut self.ui.pf_subtab);
            ui.add_space(6.0);
            match self.ui.pf_subtab {
                PortForwardSubTab::SshHosts => self.ssh_hosts_subtab(ui),
                PortForwardSubTab::SshCredentials => self.ssh_credentials_subtab(ui),
                PortForwardSubTab::Forwards => {}
            }
            return;
        }

        let parsed_value = serde_json::from_str::<serde_json::Value>(&self.editor).ok();

        let entries = parsed_value
            .as_ref()
            .map(|v| crate::port_forward::extract_entries(v, &self.ui.workspace, &self.profile_id))
            .unwrap_or_default();
        let paused = self
            .ui
            .workspace
            .paused_port_forwards
            .get(&self.profile_id)
            .cloned()
            .unwrap_or_default();
        let ssh_forwards = self.ui.ssh_state.forwards.clone();
        let ssh_hosts = self.ui.ssh_state.hosts.clone();
        let ssh_runtime = self.ui.ssh_state.runtime.clone();
        let node_names = self
            .ui
            .workspace
            .node_names
            .get(&self.profile_id)
            .cloned()
            .unwrap_or_default();
        let available_outbounds = parsed_value
            .as_ref()
            .map(|v| crate::port_forward::available_outbounds(v, &node_names))
            .unwrap_or_default();
        let inbound_conflicts: std::collections::HashMap<String, String> = parsed_value
            .as_ref()
            .and_then(|v| v.get("inbounds"))
            .and_then(serde_json::Value::as_array)
            .map(|inbounds| {
                proxy_config::inbound_port_conflicts(inbounds)
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
        let ssh_running_count = ssh_runtime
            .iter()
            .filter(|r| r.state == SshForwardState::Running)
            .count();
        let ssh_ctl_enabled = self.ui.connected && !self.busy && self.ui.uncertain.is_none();
        let default_ssh_host = ssh_hosts.first().map(|h| h.id.as_str());
        let summary_text = format!(
            "SSH 隧道 {} 条（运行中 {ssh_running_count}） · Xray 转发 {} 条（启用 {active_count}）",
            ssh_forwards.len(),
            entries.len()
        );
        let mut next_sub = self.ui.pf_subtab;
        let mut open_new_ssh = false;
        let mut open_new_xray = false;
        crate::widgets::responsive_header(
            ui,
            580.0,
            |ui| {
                render_subtabs(ui, &mut next_sub);
            },
            |ui, rtl| {
                let xray_hover = if parsed_value.is_none() {
                    "当前 Xray 草稿存在 JSON 语法错误，请先修正语法；SSH 隧道仍可正常使用"
                } else {
                    "新增 Xray (dokodemo-door) 端口转发"
                };
                for idx in if rtl { [1, 0] } else { [0, 1] } {
                    match idx {
                        0 if primary(ui, "+ SSH 隧道", ssh_ctl_enabled) => {
                            open_new_ssh = true;
                        }
                        1 if ui
                            .add_enabled(
                                enabled && parsed_value.is_some(),
                                egui::Button::new("+ Xray 转发"),
                            )
                            .on_hover_text(xray_hover)
                            .clicked() =>
                        {
                            open_new_xray = true;
                        }
                        _ => {}
                    }
                }
            },
        );
        self.ui.pf_subtab = next_sub;
        if open_new_ssh {
            self.ui.pf_form = crate::port_forward::PortForwardForm::from_new_ssh(
                parsed_value.as_ref(),
                &paused,
                &ssh_forwards,
                default_ssh_host,
            );
            self.ui.pf_error = None;
            self.ui.dialog = Some(Dialog::PortForward);
        }
        if open_new_xray && let Some(ref val) = parsed_value {
            self.ui.pf_form = crate::port_forward::PortForwardForm::from_new_with_ssh(
                val,
                &paused,
                &ssh_forwards,
            );
            self.ui.pf_error = None;
            self.ui.dialog = Some(Dialog::PortForward);
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            crate::widgets::badge(ui, &summary_text, crate::widgets::BadgeTone::Muted);
            ui.add(
                egui::Label::new(
                    RichText::new("SSH 隧道独立于 Core；Xray 转发随当前配置运行。")
                        .small()
                        .weak(),
                )
                .wrap(),
            );
        });
        ui.add_space(6.0);
        if parsed_value.is_none() {
            crate::widgets::banner_box(
                ui,
                crate::widgets::BadgeTone::Warn,
                "当前 Xray 草稿存在 JSON 语法错误，Xray 转发暂不可编辑；下方 SSH 隧道完全不受影响。请前往「更多与源码 → 原始 JSON」修正语法。",
            );
            ui.add_space(6.0);
        }

        if !ssh_forwards.is_empty() || entries.is_empty() || parsed_value.is_none() {
            let all_running = !ssh_forwards.is_empty()
                && ssh_forwards.iter().all(|f| {
                    ssh_runtime
                        .iter()
                        .find(|r| r.forward_id == f.id)
                        .is_some_and(|r| r.state == SshForwardState::Running)
                });
            let any_active = ssh_runtime.iter().any(|r| {
                matches!(
                    r.state,
                    SshForwardState::Running | SshForwardState::Connecting
                )
            });
            let mut start_all = false;
            let mut stop_all = false;
            crate::widgets::responsive_header(
                ui,
                480.0,
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("SSH 隧道").strong().size(14.5));
                        crate::widgets::badge(
                            ui,
                            "独立于 Core",
                            crate::widgets::BadgeTone::Success,
                        );
                    });
                },
                |ui, rtl| {
                    if ssh_forwards.len() >= 2 {
                        for idx in if rtl { [1, 0] } else { [0, 1] } {
                            match idx {
                                0 if ui
                                    .add_enabled(
                                        ssh_ctl_enabled && !all_running,
                                        egui::Button::new("全部启动"),
                                    )
                                    .clicked() =>
                                {
                                    start_all = true;
                                }
                                1 if ui
                                    .add_enabled(
                                        ssh_ctl_enabled && any_active,
                                        egui::Button::new("全部停止"),
                                    )
                                    .clicked() =>
                                {
                                    stop_all = true;
                                }
                                _ => {}
                            }
                        }
                    }
                },
            );
            if start_all {
                self.send(Command::StartAllSshForwards);
            }
            if stop_all {
                self.send(Command::StopAllSshForwards);
            }
            ui.add_space(6.0);

            if ssh_forwards.is_empty() {
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "暂无 SSH 隧道；点击上方「+ SSH 隧道」新增（支持粘贴 ssh -L 命令并独立按需启停）。",
                        )
                        .small()
                        .weak(),
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
            }
        }

        for fwd in &ssh_forwards {
            ui.push_id(("ssh-fwd", &fwd.id), |ui| {
                let rt = ssh_runtime.iter().find(|r| r.forward_id == fwd.id);
                let state = rt.map(|r| r.state).unwrap_or(SshForwardState::Stopped);
                let host_info = ssh_hosts.iter().find(|h| h.id == fwd.ssh_host_id);
                let missing_host = host_info.is_none();
                let border_tone = if missing_host || state == SshForwardState::Error {
                    Some(crate::widgets::BadgeTone::Danger)
                } else {
                    None
                };
                crate::widgets::subcard(ui, border_tone, state == SshForwardState::Stopped, |ui| {
                    let copy_ip = if fwd.listen == crate::port_forward::LISTEN_LAN {
                        crate::port_forward::LISTEN_LOCAL
                    } else {
                        fwd.listen.as_str()
                    };
                    let copy_addr = format!("{copy_ip}:{}", fwd.port);
                    let lan_exposed = !crate::port_forward::is_loopback_listen(&fwd.listen);
                    let mut action_idx = None;
                    crate::widgets::responsive_header(
                        ui,
                        700.0,
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(&fwd.name).strong().size(15.0));
                                crate::widgets::badge(
                                    ui,
                                    "SSH 隧道",
                                    crate::widgets::BadgeTone::Accent,
                                );
                                crate::widgets::badge(ui, "TCP", crate::widgets::BadgeTone::Muted);
                                match state {
                                    SshForwardState::Running => crate::widgets::badge(
                                        ui,
                                        "运行中",
                                        crate::widgets::BadgeTone::Success,
                                    ),
                                    SshForwardState::Connecting => crate::widgets::badge(
                                        ui,
                                        "连接中…",
                                        crate::widgets::BadgeTone::Accent,
                                    ),
                                    SshForwardState::Error => crate::widgets::badge(
                                        ui,
                                        "连接失败",
                                        crate::widgets::BadgeTone::Danger,
                                    ),
                                    SshForwardState::Stopped => crate::widgets::badge(
                                        ui,
                                        "已停止",
                                        crate::widgets::BadgeTone::Muted,
                                    ),
                                }
                                if lan_exposed {
                                    crate::widgets::badge(
                                        ui,
                                        "局域网暴露 · 无认证",
                                        crate::widgets::BadgeTone::Warn,
                                    );
                                }
                            });
                        },
                        |ui, rtl| {
                            let toggle_label = if state == SshForwardState::Running {
                                "停止"
                            } else {
                                "启动"
                            };
                            let toggle_enabled = ssh_ctl_enabled
                                && (state == SshForwardState::Running || !missing_host);
                            for idx in if rtl {
                                [4, 3, 2, 1, 0]
                            } else {
                                [0, 1, 2, 3, 4]
                            } {
                                match idx {
                                    0 if ui
                                        .add_enabled(
                                            toggle_enabled,
                                            egui::Button::new(toggle_label),
                                        )
                                        .clicked() =>
                                    {
                                        action_idx = Some(0);
                                    }
                                    1 if ui.button("复制地址").clicked() => {
                                        action_idx = Some(1)
                                    }
                                    2 if ui
                                        .add_enabled(ssh_ctl_enabled, egui::Button::new("克隆"))
                                        .clicked() =>
                                    {
                                        action_idx = Some(2);
                                    }
                                    3 if ui
                                        .add_enabled(ssh_ctl_enabled, egui::Button::new("编辑"))
                                        .clicked() =>
                                    {
                                        action_idx = Some(3);
                                    }
                                    4 if ui
                                        .add_enabled(ssh_ctl_enabled, egui::Button::new("删除"))
                                        .clicked() =>
                                    {
                                        action_idx = Some(4);
                                    }
                                    _ => {}
                                }
                            }
                        },
                    );
                    match action_idx {
                        Some(0) if state == SshForwardState::Running => {
                            self.send(Command::StopSshForward { id: fwd.id.clone() });
                        }
                        Some(0) => {
                            self.send(Command::StartSshForward { id: fwd.id.clone() });
                        }
                        Some(1) => {
                            ui.ctx().copy_text(copy_addr.clone());
                            self.message = format!("已复制监听地址：{copy_addr}");
                        }
                        Some(2) => {
                            self.ui.pf_form = crate::port_forward::PortForwardForm::from_clone_ssh(
                                fwd,
                                parsed_value.as_ref(),
                                &paused,
                                &ssh_forwards,
                            );
                            self.ui.pf_error = None;
                            self.ui.dialog = Some(Dialog::PortForward);
                        }
                        Some(3) => {
                            self.ui.pf_form =
                                crate::port_forward::PortForwardForm::from_ssh_forward_with_running(
                                    fwd,
                                    state == SshForwardState::Running,
                                );
                            self.ui.pf_error = None;
                            self.ui.dialog = Some(Dialog::PortForward);
                        }
                        Some(4) => {
                            self.send(Command::DeleteSshForward { id: fwd.id.clone() });
                        }
                        _ => {}
                    }
                    let host_label = host_info
                        .map(|h| format!("SSH · {} ({}:{})", h.name, h.host, h.port))
                        .unwrap_or_else(|| "跳板机已删除".into());
                    crate::widgets::flow_strip(ui, |ui| {
                        ui.monospace(format!("{}:{}", fwd.listen, fwd.port));
                        ui.label(RichText::new("-> 经由").small().weak());
                        crate::widgets::badge(
                            ui,
                            &host_label,
                            if missing_host {
                                crate::widgets::BadgeTone::Danger
                            } else {
                                crate::widgets::BadgeTone::Accent
                            },
                        );
                        ui.label(RichText::new("-> 目标").small().weak());
                        ui.monospace(format!("{}:{}", fwd.target_host, fwd.target_port));
                    });
                    if let Some(err) = rt.and_then(|r| r.last_error.as_deref()) {
                        crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Danger, err);
                    }
                });
                ui.add_space(8.0);
            });
        }

        let Some(mut value) = parsed_value else {
            return;
        };

        ui.add_space(4.0);
        ui.label(
            RichText::new("Xray 端口转发（dokodemo-door，随当前配置下发）")
                .strong()
                .size(14.5),
        );
        ui.add_space(6.0);

        if entries.is_empty() {
            crate::widgets::subcard(ui, None, false, |ui| {
                ui.label(
                    RichText::new("当前配置暂无 Xray 端口转发规则。点击上方「+ Xray 转发」新增。")
                        .weak(),
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
                    let copy_ip = if entry.listen == crate::port_forward::LISTEN_LAN {
                        crate::port_forward::LISTEN_LOCAL
                    } else {
                        entry.listen.as_str()
                    };
                    let copy_addr = format!("{copy_ip}:{}", entry.port);
                    let title = if entry.name.is_empty() {
                        format!("转发 {} -> {}", entry.port, entry.target_port)
                    } else {
                        entry.name.clone()
                    };
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
                    let mut toggle_active: Option<bool> = None;
                    let mut xray_action = None;

                    crate::widgets::responsive_header(
                        ui,
                        700.0,
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let mut active = entry.enabled;
                                if ui
                                    .add_enabled(enabled, egui::Checkbox::new(&mut active, ""))
                                    .on_hover_text("取消勾选可保留参数但暂停该端口监听")
                                    .changed()
                                {
                                    toggle_active = Some(active);
                                }
                                ui.label(RichText::new(&title).strong().size(15.0));
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
                            });
                        },
                        |ui, rtl| {
                            for idx in if rtl { [2, 1, 0] } else { [0, 1, 2] } {
                                match idx {
                                    0 if ui.button("复制监听地址").clicked() => {
                                        xray_action = Some(0);
                                    }
                                    1 if ui
                                        .add_enabled(enabled, egui::Button::new("编辑"))
                                        .clicked() =>
                                    {
                                        xray_action = Some(1);
                                    }
                                    2 if ui
                                        .add_enabled(enabled, egui::Button::new("删除"))
                                        .clicked() =>
                                    {
                                        xray_action = Some(2);
                                    }
                                    _ => {}
                                }
                            }
                        },
                    );
                    if let Some(active) = toggle_active {
                        match crate::port_forward::set_entry_enabled(
                            &mut value,
                            &mut self.ui.workspace,
                            &self.profile_id,
                            &entry,
                            active,
                        ) {
                            Ok(()) => {
                                json_changed = true;
                                ws_changed = true;
                                self.message = if active {
                                    format!(
                                        "已启用端口转发：{}（请点击底栏「检查并应用」生效）",
                                        entry.tag
                                    )
                                } else {
                                    format!("已停用端口转发：{}（参数已保留于工作区）", entry.tag)
                                };
                            }
                            Err(err) => self.message = err,
                        }
                    }
                    match xray_action {
                        Some(0) => {
                            ui.ctx().copy_text(copy_addr.clone());
                            self.message = format!("已复制监听地址：{copy_addr}");
                        }
                        Some(1) => {
                            self.ui.pf_form =
                                crate::port_forward::PortForwardForm::from_entry(&entry);
                            self.ui.pf_error = None;
                            self.ui.dialog = Some(Dialog::PortForward);
                        }
                        Some(2) => {
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
                        _ => {}
                    }

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
        if ssh_forwards.is_empty() {
            ui.label(
                RichText::new("提示：上方「+ SSH 隧道」支持粘贴 ssh -L 命令并独立按需启停。")
                    .small()
                    .weak(),
            );
        }
    }

    fn ssh_hosts_subtab(&mut self, ui: &mut egui::Ui) {
        let ctl_ok = self.ui.connected && !self.busy && self.ui.uncertain.is_none();
        let hosts = self.ui.ssh_state.hosts.clone();
        let credentials = self.ui.ssh_state.credentials.clone();
        let forwards = self.ui.ssh_state.forwards.clone();
        crate::widgets::subcard(ui, None, true, |ui| {
            let mut open_new_host = false;
            crate::widgets::responsive_header(
                ui,
                520.0,
                |ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("SSH 跳板机管理").strong().size(15.5));
                        ui.add(
                            egui::Label::new(
                                RichText::new(
                                    "管理跳板机地址、关联凭证与 Host Key 指纹（多条隧道可复用）。",
                                )
                                .small()
                                .weak(),
                            )
                            .wrap(),
                        );
                    });
                },
                |ui, _rtl| {
                    if primary(ui, "+ 新增 SSH 跳板机", ctl_ok) {
                        open_new_host = true;
                    }
                },
            );
            if open_new_host {
                self.ui.ssh_host_form = ui::SshHostForm::for_new(&credentials, false);
                self.ui.dialog = Some(Dialog::SshHost);
            }
        });
        ui.add_space(8.0);
        if hosts.is_empty() {
            crate::widgets::subcard(ui, None, false, |ui| {
                ui.label(RichText::new("尚未配置 SSH 跳板机。").weak());
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "提示：点击右上角「+ 新增 SSH 跳板机」可直接在同一弹窗内填写新凭证，无需先跳转「SSH 凭证」。",
                        )
                        .small()
                        .weak(),
                    )
                    .wrap(),
                );
            });
            return;
        }
        let parsed_editor = serde_json::from_str::<serde_json::Value>(&self.editor).ok();
        let paused_pf = self
            .ui
            .workspace
            .paused_port_forwards
            .get(&self.profile_id)
            .cloned()
            .unwrap_or_default();
        for host in &hosts {
            ui.push_id(("ssh-host-card", &host.id), |ui| {
                let cred = credentials.iter().find(|c| c.id == host.credential_id);
                let ref_count = forwards.iter().filter(|f| f.ssh_host_id == host.id).count();
                let missing_fp = host.host_key_fingerprint.trim().is_empty();
                let test_status = self.ui.ssh_host_tests.get(&host.id).cloned();
                let is_testing = matches!(test_status, Some(ui::SshHostTestStatus::Testing));
                let border = if cred.is_none() || missing_fp {
                    Some(crate::widgets::BadgeTone::Warn)
                } else {
                    None
                };
                crate::widgets::subcard(ui, border, false, |ui| {
                    let mut host_action = None;
                    crate::widgets::responsive_header(
                        ui,
                        680.0,
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(&host.name).strong().size(15.0));
                                crate::widgets::mono_badge(
                                    ui,
                                    &format!("{}:{}", host.host, host.port),
                                    crate::widgets::BadgeTone::Accent,
                                );
                                if missing_fp {
                                    crate::widgets::badge(
                                        ui,
                                        "未验证指纹",
                                        crate::widgets::BadgeTone::Warn,
                                    );
                                } else {
                                    crate::widgets::badge(
                                        ui,
                                        "指纹已固定",
                                        crate::widgets::BadgeTone::Success,
                                    );
                                }
                                if ref_count > 0 {
                                    crate::widgets::badge(
                                        ui,
                                        &format!("被 {ref_count} 条转发引用"),
                                        crate::widgets::BadgeTone::Muted,
                                    );
                                }
                            });
                        },
                        |ui, rtl| {
                            let test_label = if is_testing {
                                "测试中…"
                            } else {
                                "测试连接"
                            };
                            for idx in if rtl { [3, 2, 1, 0] } else { [0, 1, 2, 3] } {
                                match idx {
                                    0 if ui
                                        .add_enabled(ctl_ok, egui::Button::new("+ 转发"))
                                        .on_hover_text("使用此跳板机新建一条 SSH 本地隧道")
                                        .clicked() =>
                                    {
                                        host_action = Some(0);
                                    }
                                    1 if ui
                                        .add_enabled(
                                            ctl_ok && !is_testing,
                                            egui::Button::new(test_label),
                                        )
                                        .clicked() =>
                                    {
                                        host_action = Some(1);
                                    }
                                    2 if ui
                                        .add_enabled(ctl_ok, egui::Button::new("编辑"))
                                        .clicked() =>
                                    {
                                        host_action = Some(2);
                                    }
                                    3 if ui
                                        .add_enabled(
                                            ctl_ok && ref_count == 0,
                                            egui::Button::new("删除"),
                                        )
                                        .on_hover_text(if ref_count > 0 {
                                            "请先删除或修改引用此跳板机的转发规则"
                                        } else {
                                            "删除此 SSH 跳板机"
                                        })
                                        .clicked() =>
                                    {
                                        host_action = Some(3);
                                    }
                                    _ => {}
                                }
                            }
                        },
                    );
                    match host_action {
                        Some(0) => {
                            self.ui.pf_form = crate::port_forward::PortForwardForm::from_new_ssh(
                                parsed_editor.as_ref(),
                                &paused_pf,
                                &forwards,
                                Some(&host.id),
                            );
                            self.ui.pf_error = None;
                            self.ui.dialog = Some(Dialog::PortForward);
                        }
                        Some(1) => {
                            self.ui
                                .ssh_host_tests
                                .insert(host.id.clone(), ui::SshHostTestStatus::Testing);
                            self.send(Command::TestSshHost {
                                host_id: host.id.clone(),
                                host: None,
                                inline_credential: None,
                            });
                        }
                        Some(2) => {
                            self.ui.ssh_host_form = ui::SshHostForm {
                                editing_id: Some(host.id.clone()),
                                name: host.name.clone(),
                                host: host.host.clone(),
                                port: host.port.to_string(),
                                cred_mode: ui::SshHostCredMode::Reuse,
                                credential_id: host.credential_id.clone(),
                                host_key_fingerprint: host.host_key_fingerprint.clone(),
                                ..Default::default()
                            };
                            self.ui.dialog = Some(Dialog::SshHost);
                        }
                        Some(3) => {
                            self.send(Command::DeleteSshHost {
                                id: host.id.clone(),
                            });
                        }
                        _ => {}
                    }
                    if let Some(c) = cred {
                        line(
                            ui,
                            "关联凭证",
                            &format!("{}（账号：{}）", c.name, c.username),
                        );
                    } else {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            "关联凭证不存在，请编辑重新指定",
                        );
                    }
                    if !missing_fp {
                        line(ui, "Host Key 指纹", &host.host_key_fingerprint);
                    }
                    if let Some(status) = &test_status {
                        ui.add_space(4.0);
                        match status {
                            ui::SshHostTestStatus::Testing => {
                                crate::widgets::banner_box(
                                    ui,
                                    crate::widgets::BadgeTone::Accent,
                                    "正在测试 SSH 握手与身份认证…",
                                );
                            }
                            ui::SshHostTestStatus::Ok(msg) => {
                                crate::widgets::banner_box(
                                    ui,
                                    crate::widgets::BadgeTone::Success,
                                    msg,
                                );
                            }
                            ui::SshHostTestStatus::Error(msg) => {
                                crate::widgets::banner_box(
                                    ui,
                                    crate::widgets::BadgeTone::Warn,
                                    msg,
                                );
                            }
                        }
                    }
                });
                ui.add_space(8.0);
            });
        }
    }

    fn ssh_credentials_subtab(&mut self, ui: &mut egui::Ui) {
        let ctl_ok = self.ui.connected && !self.busy && self.ui.uncertain.is_none();
        let credentials = self.ui.ssh_state.credentials.clone();
        let hosts = self.ui.ssh_state.hosts.clone();
        crate::widgets::subcard(ui, None, true, |ui| {
            let mut open_new_cred = false;
            crate::widgets::responsive_header(
                ui,
                520.0,
                |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("SSH 凭证管理（账号与密码 / 私钥）")
                                .strong()
                                .size(15.5),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(
                                    "管理 SSH 账号、密码与私钥（加密保存、永不回显，可跨跳板机复用）。",
                                )
                                .small()
                                .weak(),
                            )
                            .wrap(),
                        );
                    });
                },
                |ui, _rtl| {
                    if primary(ui, "+ 新增 SSH 凭证", ctl_ok) {
                        open_new_cred = true;
                    }
                },
            );
            if open_new_cred {
                self.ui.ssh_cred_form = Default::default();
                self.ui.dialog = Some(Dialog::SshCredential);
            }
        });
        ui.add_space(8.0);
        if credentials.is_empty() {
            crate::widgets::subcard(ui, None, false, |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "尚未创建任何 SSH 凭证。点击右上角「+ 新增 SSH 凭证」，或在新建跳板机时直接填写凭证。",
                        )
                        .weak(),
                    )
                    .wrap(),
                );
            });
            return;
        }
        for cred in &credentials {
            ui.push_id(("ssh-cred-card", &cred.id), |ui| {
                let ref_count = hosts.iter().filter(|h| h.credential_id == cred.id).count();
                crate::widgets::subcard(ui, None, false, |ui| {
                    let mut cred_action = None;
                    crate::widgets::responsive_header(
                        ui,
                        600.0,
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(&cred.name).strong().size(15.0));
                                crate::widgets::mono_badge(
                                    ui,
                                    &format!("账号: {}", cred.username),
                                    crate::widgets::BadgeTone::Accent,
                                );
                                match cred.auth_kind {
                                    SshAuthKind::Password => {
                                        crate::widgets::badge(
                                            ui,
                                            if cred.has_password {
                                                "密码认证（已加密保存）"
                                            } else {
                                                "密码认证（未设置密码）"
                                            },
                                            if cred.has_password {
                                                crate::widgets::BadgeTone::Success
                                            } else {
                                                crate::widgets::BadgeTone::Warn
                                            },
                                        );
                                    }
                                    SshAuthKind::PrivateKey => {
                                        crate::widgets::badge(
                                            ui,
                                            if cred.has_passphrase {
                                                "私钥认证（含加密口令）"
                                            } else {
                                                "私钥认证"
                                            },
                                            crate::widgets::BadgeTone::Success,
                                        );
                                    }
                                }
                                if ref_count > 0 {
                                    crate::widgets::badge(
                                        ui,
                                        &format!("被 {ref_count} 台跳板机引用"),
                                        crate::widgets::BadgeTone::Muted,
                                    );
                                }
                            });
                        },
                        |ui, rtl| {
                            for idx in if rtl { [1, 0] } else { [0, 1] } {
                                match idx {
                                    0 if ui
                                        .add_enabled(ctl_ok, egui::Button::new("编辑"))
                                        .clicked() =>
                                    {
                                        cred_action = Some(0);
                                    }
                                    1 if ui
                                        .add_enabled(
                                            ctl_ok && ref_count == 0,
                                            egui::Button::new("删除"),
                                        )
                                        .on_hover_text(if ref_count > 0 {
                                            "请先修改或删除引用此凭证的跳板机"
                                        } else {
                                            "删除此 SSH 凭证"
                                        })
                                        .clicked() =>
                                    {
                                        cred_action = Some(1);
                                    }
                                    _ => {}
                                }
                            }
                        },
                    );
                    match cred_action {
                        Some(0) => {
                            self.ui.ssh_cred_form = ui::SshCredentialForm {
                                editing_id: Some(cred.id.clone()),
                                name: cred.name.clone(),
                                username: cred.username.clone(),
                                auth_kind: cred.auth_kind,
                                has_existing_password: cred.has_password,
                                private_key_path: cred
                                    .private_key_path
                                    .to_string_lossy()
                                    .into_owned(),
                                has_existing_passphrase: cred.has_passphrase,
                                ..Default::default()
                            };
                            self.ui.dialog = Some(Dialog::SshCredential);
                        }
                        Some(1) => {
                            self.send(Command::DeleteSshCredential {
                                id: cred.id.clone(),
                            });
                        }
                        _ => {}
                    }
                    if cred.auth_kind == SshAuthKind::PrivateKey
                        && !cred.private_key_path.as_os_str().is_empty()
                    {
                        line(ui, "私钥文件", &display_path(&cred.private_key_path));
                    }
                });
                ui.add_space(8.0);
            });
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
                proxy_config::inbound_port_conflicts(inbounds)
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default();

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("系统代理 / 本地代理入口").strong());
            if ui
                .add_enabled(enabled, egui::Button::new("+ 添加 HTTP 入口"))
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
                .add_enabled(enabled, egui::Button::new("+ 添加 SOCKS 入口"))
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
                                    if ui.button("前往端口转发").clicked() {
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
                                if ui.add_enabled(enabled, egui::Button::new("删除")).clicked() {
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
        let before = (self.mode, self.http_tag.clone());
        let mut next_mode = self.mode;
        let mut next_http = self.http_tag.clone();
        let listeners = self.compatible_http_listeners();
        if matches!(next_mode, ProxyMode::Global | ProxyMode::Pac)
            && !listeners.iter().any(|(tag, _)| tag == &next_http)
            && let Some((first, _)) = listeners.first()
        {
            next_http = first.clone();
        }
        ui.horizontal_wrapped(|ui| {
            render_proxy_draft_controls(ui, &listeners, &mut next_mode, &mut next_http, false);
        });
        self.apply_proxy_draft_selection(before, next_mode, next_http);
    }

    fn apply_proxy_draft_selection(
        &mut self,
        before: (ProxyMode, String),
        next_mode: ProxyMode,
        next_http: String,
    ) {
        self.mode = next_mode;
        self.http_tag = next_http;
        if before.0 != self.mode {
            let profile_id = self.profile_id.clone();
            self.save_profile_proxy_mode(&profile_id, self.mode);
        }
        if before != (self.mode, self.http_tag.clone()) {
            self.invalidate_candidate();
        }
    }
}

fn render_proxy_draft_controls(
    ui: &mut egui::Ui,
    listeners: &[(String, String)],
    mode: &mut ProxyMode,
    http_tag: &mut String,
    rtl: bool,
) {
    let has_http = !listeners.is_empty();
    let mut render_http = |ui: &mut egui::Ui| {
        if listeners.len() > 1 {
            let selected = listeners
                .iter()
                .find(|(tag, _)| tag == http_tag)
                .map(|(_, label)| label.clone())
                .unwrap_or_else(|| "请选择本地 HTTP 入口".into());
            let combo_w = if ui.max_rect().width().is_finite() {
                (ui.max_rect().width() - 24.0).clamp(130.0, 210.0)
            } else {
                190.0
            };
            egui::ComboBox::from_id_salt("proxy-http-listener")
                .width(combo_w)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (tag, label) in listeners {
                        ui.selectable_value(http_tag, tag.clone(), label);
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
    };
    let modes = [
        (ProxyMode::Direct, "直连"),
        (ProxyMode::Pac, "PAC"),
        (ProxyMode::Global, "全局"),
    ];
    let mut render_modes = |ui: &mut egui::Ui| {
        for idx in if rtl { [2, 1, 0] } else { [0, 1, 2] } {
            let (target_mode, short_label) = modes[idx];
            let allowed = target_mode == ProxyMode::Direct || has_http;
            if crate::widgets::seg_button(ui, short_label, *mode == target_mode, allowed).clicked()
            {
                *mode = target_mode;
            }
        }
    };
    if rtl {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            render_http(ui);
            render_modes(ui);
            ui.label(RichText::new("代理：").small().weak());
        });
    } else {
        ui.label(RichText::new("代理方式：").small().weak());
        render_modes(ui);
        render_http(ui);
    }
}

fn profile_action_buttons(
    ui: &mut egui::Ui,
    enabled: bool,
    width: f32,
    reverse: bool,
) -> Option<usize> {
    let mut clicked = None;
    let labels = ["新建", "导入", "刷新"];
    let order = if reverse { [2, 1, 0] } else { [0, 1, 2] };
    for idx in order {
        let label = labels[idx];
        if ui
            .add_enabled(
                enabled || label == "刷新",
                egui::Button::new(label).min_size(egui::vec2(width, CONTROL_HEIGHT)),
            )
            .clicked()
        {
            clicked = Some(idx);
        }
    }
    clicked
}
