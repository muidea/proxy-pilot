use super::*;

impl App {
    pub(in crate::app) fn render(&mut self, ctx: &egui::Context) {
        if self.ui.settings_loaded
            && self.ui.profiles_loaded
            && self.ui.connected
            && !self.ui.setup_initialized
        {
            self.ui.setup_initialized = true;
            self.ui.onboarding = self.core_path.is_empty()
                || self.profiles.is_empty()
                || self.committed_id().is_none();
            self.ui.setup_step = if self.core_path.is_empty() {
                0
            } else if self.profiles.is_empty() {
                1
            } else {
                2
            };
        }
        let running_pf_count = if self.is_running() {
            self.status
                .as_ref()
                .and_then(|s| s.generation.as_ref())
                .and_then(|g| {
                    self.ui
                        .runtime
                        .as_ref()
                        .filter(|r| r.generation_id == g.id)
                        .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.json).ok())
                })
                .map(|v| crate::port_forward::dokodemo_tags(&v).len())
                .unwrap_or(0)
        } else {
            0
        };
        let running_ssh_count = self
            .ui
            .ssh_state
            .runtime
            .iter()
            .filter(|r| r.state == SshForwardState::Running)
            .count();
        let total_running_pf = running_pf_count + running_ssh_count;
        let running_profile_name = self
            .status
            .as_ref()
            .and_then(|s| s.generation.as_ref())
            .map(|g| self.ui.workspace.name(&g.candidate.profile_id).to_owned())
            .unwrap_or_default();
        let top_bar_fill = if ctx.style().visuals.dark_mode {
            Color32::from_rgb(30, 40, 55)
        } else {
            Color32::from_rgb(248, 250, 253)
        };
        let top_bar_stroke = if ctx.style().visuals.dark_mode {
            Color32::from_rgb(43, 55, 73)
        } else {
            Color32::from_rgb(223, 229, 239)
        };
        egui::TopBottomPanel::top("window-bar")
            .frame(
                egui::Frame::new()
                    .fill(top_bar_fill)
                    .stroke(egui::Stroke::new(1.0, top_bar_stroke))
                    .inner_margin(egui::Margin::symmetric(18, 10)),
            )
            .show(ctx, |ui| {
                let mut next_page = self.ui.page;
                let connected = self.ui.connected;
                let running = self.is_running();
                let mode_short = self
                    .status
                    .as_ref()
                    .and_then(|s| s.generation.as_ref())
                    .map(|g| match g.proxy_mode {
                        ProxyMode::Direct => "直连",
                        ProxyMode::Global => "全局",
                        ProxyMode::Pac => "PAC",
                    })
                    .unwrap_or("直连");
                let render_brand = |ui: &mut egui::Ui| {
                    crate::widgets::status_dot(ui, connected && running);
                    ui.label(RichText::new("Proxy Pilot").strong().size(16.0));
                    if !connected {
                        crate::widgets::badge(ui, "后台未连接", crate::widgets::BadgeTone::Warn);
                    } else if running {
                        let short_prof = crate::widgets::ellipsize(&running_profile_name, 18);
                        let prof_part = if short_prof.is_empty() {
                            format!("{mode_short}模式")
                        } else {
                            format!("{short_prof} ({mode_short})")
                        };
                        let base_text = if total_running_pf > 0 {
                            format!("Core 运行中 · {prof_part} · {total_running_pf} 条转发监听")
                        } else {
                            format!("Core 运行中 · {prof_part}")
                        };
                        let avail = self
                            .status
                            .as_ref()
                            .and_then(|s| s.proxy_availability.as_ref());
                        let (status_text, status_tone) = match avail {
                            Some(a)
                                if a.state == ProxyAvailabilityState::Unavailable
                                    && !a.checking =>
                            {
                                (
                                    format!("{base_text} · 代理不可用"),
                                    crate::widgets::BadgeTone::Danger,
                                )
                            }
                            Some(a)
                                if a.state == ProxyAvailabilityState::Degraded && !a.checking =>
                            {
                                (
                                    format!("{base_text} · 代理波动"),
                                    crate::widgets::BadgeTone::Warn,
                                )
                            }
                            Some(a)
                                if a.state == ProxyAvailabilityState::DirectOnly && !a.checking =>
                            {
                                (
                                    format!("{base_text} · 未走代理"),
                                    crate::widgets::BadgeTone::Warn,
                                )
                            }
                            _ => (base_text, crate::widgets::BadgeTone::Success),
                        };
                        crate::widgets::badge(ui, &status_text, status_tone);
                    } else if running_ssh_count > 0 {
                        crate::widgets::badge(
                            ui,
                            &format!("Core 已停止 · {running_ssh_count} 条 SSH 隧道独立运行中"),
                            crate::widgets::BadgeTone::Accent,
                        );
                    } else {
                        crate::widgets::badge(
                            ui,
                            "Core 已停止 · 未接管代理与转发端口",
                            crate::widgets::BadgeTone::Muted,
                        );
                    }
                };
                let render_version = |ui: &mut egui::Ui| {
                    ui.label(
                        RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .small()
                            .weak(),
                    );
                };
                let render_nav = |ui: &mut egui::Ui, page_state: &mut Page, reverse: bool| {
                    let tabs = [
                        (Page::Overview, "总览"),
                        (Page::Profiles, "配置"),
                        (Page::Pac, "PAC"),
                        (Page::Logs, "日志"),
                        (Page::Diagnostics, "诊断"),
                        (Page::Settings, "设置"),
                    ];
                    let iter: Box<dyn Iterator<Item = &(Page, &str)>> = if reverse {
                        Box::new(tabs.iter().rev())
                    } else {
                        Box::new(tabs.iter())
                    };
                    for &(page, title) in iter {
                        if crate::widgets::nav_tab(ui, title, *page_state == page).clicked() {
                            *page_state = page;
                        }
                    }
                };
                if ui.available_width() >= 960.0 {
                    egui::Sides::new()
                        .shrink_left()
                        .show(ui, render_brand, |ui| {
                            render_version(ui);
                            ui.add_space(4.0);
                            render_nav(ui, &mut next_page, true);
                        });
                } else {
                    egui::Sides::new()
                        .shrink_left()
                        .show(ui, render_brand, render_version);
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        render_nav(ui, &mut next_page, false);
                    });
                }
                self.ui.page = next_page;
            });
        let has_message = !self.message.trim().is_empty()
            && !matches!(self.message.as_str(), "后台已连接。" | "正在连接后台…");
        if has_message || self.busy || self.shutdown_requested || self.dependency_job.is_some() {
            egui::TopBottomPanel::bottom("feedback")
                .frame(
                    egui::Frame::new()
                        .fill(ctx.style().visuals.window_fill())
                        .inner_margin(egui::Margin::symmetric(20, 10)),
                )
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if self.busy || self.dependency_job.is_some() {
                            ui.spinner();
                        }
                        if self.shutdown_requested {
                            ui.label(if !self.ui.connected || self.ui.uncertain.is_some() {
                                "正在确认后台连接与原操作结果，确认后将安全退出。"
                            } else {
                                "完成当前操作后，将停止 Core、恢复代理并退出。"
                            });
                        } else if self.busy {
                            ui.label("操作正在执行，仍可浏览页面与日志。");
                        } else {
                            ui.add(
                                egui::Label::new(RichText::new(&self.message).small()).truncate(),
                            );
                        }
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                let content_height = ui.available_height() - 20.0;
                egui::ScrollArea::vertical()
                    .id_salt("main-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        // Keep scroll documents unbounded, even when banners are
                        // taller than the viewport. Editors receive a size budget.
                        let content_top = ui.cursor().top();
                        self.banners(ui);
                        let page_height =
                            (content_height - (ui.cursor().top() - content_top)).max(0.0);
                        if self.ui.page == Page::Overview && self.ui.onboarding {
                            self.setup(ui);
                        } else {
                            match self.ui.page {
                                Page::Overview => self.overview(ui),
                                Page::Profiles => self.profiles_page(ui, page_height),
                                Page::Pac => self.pac_page(ui),
                                Page::Diagnostics => self.diagnostics(ui),
                                Page::Logs => self.logs_page(ui, page_height),
                                Page::Settings => self.settings_page(ui),
                            }
                        }
                        ui.add_space(20.0);
                    });
            });
        self.dialog(ctx);
        if self.ui.dialog.is_none() && self.ui.preview_open {
            self.preview(ctx);
        }
    }
    pub(in crate::app) fn set_appearance(&mut self, appearance: Appearance, ctx: &egui::Context) {
        self.appearance = appearance;
        appearance.apply(ctx);
        if self.ui.persistence
            && let Err(error) = appearance.save(&self.root)
        {
            self.fail(error, String::new());
        }
    }
    pub(in crate::app) fn banners(&mut self, ui: &mut egui::Ui) {
        if !self.ui.connected {
            card(ui, |ui| {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "后台未连接 · 当前展示最后一次已知状态",
                );
                ui.label(
                    "进程和系统代理的最新状态尚未确认。可继续查看已载入内容，写操作暂不可用。",
                );
                if ui.button("重新连接并读取状态").clicked() {
                    self.send(Command::GetStatus);
                    if !self.ui.settings_loaded {
                        self.send(Command::GetSettings);
                    }
                    if !self.ui.profiles_loaded {
                        self.refresh_profiles();
                    }
                }
            });
            ui.add_space(14.0);
        }
        if let Some(request) = self.ui.uncertain.as_ref().map(|u| u.request_id.clone()) {
            card(ui, |ui| {
                ui.colored_label(ui.visuals().warn_fg_color, "操作结果待确认，请勿重复提交");
                ui.label("后台响应中断，当前状态不能证明原操作是否已经完成。");
                ui.monospace(&request);
                if ui.button("查询原操作结果").clicked() {
                    self.send(Command::GetOperation {
                        request_id: request,
                    });
                }
            });
            ui.add_space(14.0);
        }
        if self.status.as_ref().is_some_and(|s| s.recovery_required) {
            card(ui, |ui| {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "存在未完成的恢复，普通变更已暂停",
                );
                ui.label("先检查下方错误与操作证据。涉及外部代理变更时，保留其他程序的设置，不自动覆盖。");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("重新查询恢复状态").clicked() {
                        self.send(Command::GetStatus);
                    }
                    if ui.button("查看诊断").clicked() {
                        self.ui.page = Page::Diagnostics;
                    }
                    if ui
                        .add_enabled(
                            self.ui.connected && !self.busy && self.ui.uncertain.is_none(),
                            egui::Button::new("安全停止 Core"),
                        )
                        .clicked()
                    {
                        self.ui.dialog = Some(Dialog::Confirm(Action::Stop));
                    }
                    if self.can_resolve_proxy_conflict()
                        && ui.button("保留当前系统代理并解除冲突…").clicked()
                    {
                        self.ui.dialog = Some(Dialog::Confirm(Action::ResolveProxyConflict));
                    }
                    if self.can_resolve_startup_conflict()
                        && ui.button("重新接管自启计划任务…").clicked()
                    {
                        self.ui.dialog = Some(Dialog::Confirm(Action::ResolveStartupConflict));
                    }
                });
                if let Some(error) = self.status.as_ref().and_then(|s| s.last_error.as_ref()) {
                    egui::CollapsingHeader::new("恢复证据与技术详情").show(ui, |ui| {
                        ui.monospace(error.to_string());
                    });
                }
            });
            ui.add_space(14.0);
        }
        if let Some(failure) = &self.ui.failure {
            let startup_conflict =
                failure.error.code == "STARTUP_CONFLICT" && self.can_resolve_startup_conflict();
            card(ui, |ui| {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    error_advice(&failure.error.code),
                );
                if startup_conflict && ui.button("重新接管自启计划任务…").clicked() {
                    self.ui.dialog = Some(Dialog::Confirm(Action::ResolveStartupConflict));
                }
                egui::CollapsingHeader::new("错误详情与请求身份").show(ui, |ui| {
                    ui.monospace(failure.error.to_string());
                    if !failure.request.is_empty() {
                        ui.monospace(format!("request_id: {}", failure.request));
                    }
                });
            });
            ui.add_space(14.0);
        } else if let Some(error) = self
            .status
            .as_ref()
            .filter(|s| !s.recovery_required)
            .and_then(|s| s.last_error.as_ref())
        {
            card(ui, |ui| {
                ui.colored_label(ui.visuals().warn_fg_color, "后台最近记录的错误");
                ui.label(error_advice(&error.code));
                egui::CollapsingHeader::new("后台错误详情").show(ui, |ui| {
                    ui.monospace(error.to_string());
                });
            });
            ui.add_space(14.0);
        }
    }
    pub(in crate::app) fn overview(&mut self, ui: &mut egui::Ui) {
        let running = self.is_running();
        let enabled = self.write_blocker().is_none();
        let generation = self.status.as_ref().and_then(|s| s.generation.clone());
        heading(
            ui,
            "总览",
            "直观查看 Core 连接状态、系统代理方式、生效 PAC 与端口转发运行情况。",
        );
        if let Some(notice) = self
            .status
            .as_ref()
            .and_then(|status| status.auto_connect_notice.as_deref())
        {
            crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Warn, notice);
            ui.add_space(12.0);
        } else {
            ui.add_space(2.0);
        }
        let runtime = self
            .ui
            .runtime
            .as_ref()
            .filter(|r| generation.as_ref().is_some_and(|g| g.id == r.generation_id));
        let active_forwards: Vec<crate::port_forward::PortForwardEntry> = if running {
            generation
                .as_ref()
                .zip(runtime.and_then(|r| serde_json::from_str::<serde_json::Value>(&r.json).ok()))
                .map(|(g, v)| {
                    crate::port_forward::extract_entries(
                        &v,
                        &self.ui.workspace,
                        &g.candidate.profile_id,
                    )
                    .into_iter()
                    .filter(|e| e.enabled)
                    .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let pf_count = active_forwards.len();
        let running_ssh_count = self
            .ui
            .ssh_state
            .runtime
            .iter()
            .filter(|r| r.state == SshForwardState::Running)
            .count();
        let endpoint = runtime
            .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.json).ok())
            .and_then(|v| {
                v.get("inbounds")?
                    .as_array()?
                    .iter()
                    .find(|i| i.get("protocol").and_then(|v| v.as_str()) == Some("http"))
                    .cloned()
            })
            .map(|i| {
                format!(
                    "{}:{}",
                    i.get("listen")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0.0.0.0"),
                    i.get("port").map(|v| v.to_string()).unwrap_or_default()
                )
            })
            .unwrap_or_else(|| "无已读取的 HTTP 入口".into());
        let name = generation
            .as_ref()
            .map(|g| self.ui.workspace.name(&g.candidate.profile_id).to_string())
            .unwrap_or_else(|| "无运行配置".into());
        let proxy = self.proxy_label();
        let owned = self
            .status
            .as_ref()
            .is_some_and(|s| s.proxy_state == "Owned");
        let committed = self
            .status
            .as_ref()
            .and_then(|s| s.generation.as_ref())
            .map(|g| g.proxy_mode);
        let wide = ui.available_width() >= 860.0;
        let render_connection_card = |app: &mut App, ui: &mut egui::Ui| {
            card(ui, |ui| {
                let mut open_profiles = false;
                let mut open_diag_page = false;
                crate::widgets::responsive_header(
                    ui,
                    360.0,
                    |ui| {
                        ui.label(RichText::new("连接与系统代理状态").strong().size(16.0));
                    },
                    |ui, rtl| {
                        let primary_is_profiles = running || generation.is_some();
                        for idx in if rtl { [1, 0] } else { [0, 1] } {
                            match idx {
                                0 if primary_is_profiles => {
                                    if primary(ui, "管理配置", true) {
                                        open_profiles = true;
                                    }
                                }
                                0 => {
                                    if primary(ui, "选择配置", enabled) {
                                        open_profiles = true;
                                    }
                                }
                                1 if ui.button("路由诊断").clicked() => {
                                    open_diag_page = true;
                                }
                                _ => {}
                            }
                        }
                    },
                );
                if open_profiles {
                    app.ui.page = Page::Profiles;
                }
                if open_diag_page {
                    app.ui.page = Page::Diagnostics;
                }
                ui.separator();

                let (core_label, core_tone) = if !app.ui.connected {
                    ("状态待确认", crate::widgets::BadgeTone::Warn)
                } else if running {
                    ("Core 运行中", crate::widgets::BadgeTone::Success)
                } else if app
                    .status
                    .as_ref()
                    .is_some_and(|s| s.core_state == "Degraded")
                {
                    ("Core 等待恢复", crate::widgets::BadgeTone::Warn)
                } else {
                    ("Core 已停止", crate::widgets::BadgeTone::Muted)
                };
                let avail = app
                    .status
                    .as_ref()
                    .and_then(|s| s.proxy_availability.as_ref());
                let (avail_badge, avail_tone, avail_hint, can_retest, avail_hover) = match avail {
                    Some(a) if a.checking => (
                        "检测中…".to_owned(),
                        crate::widgets::BadgeTone::Accent,
                        String::new(),
                        false,
                        format!("正在检测：{}", a.target_url),
                    ),
                    Some(a) => {
                        let tone = availability_tone(a.state);
                        let retest_ok = running
                            && a.state != ProxyAvailabilityState::Unsupported
                            && app.ui.connected
                            && !app.shutdown_requested;
                        (
                            if a.badge_text.is_empty() {
                                "待检测".to_owned()
                            } else {
                                a.badge_text.clone()
                            },
                            tone,
                            a.overview_hint.clone(),
                            retest_ok,
                            a.headline.clone(),
                        )
                    }
                    None => (
                        if running { "待检测" } else { "未运行" }.to_owned(),
                        crate::widgets::BadgeTone::Muted,
                        String::new(),
                        running && app.ui.connected && !app.shutdown_requested,
                        String::new(),
                    ),
                };

                let stop_enabled = app.ui.connected && !app.busy && app.ui.uncertain.is_none();
                let mut core_action: Option<Action> = None;
                crate::widgets::responsive_compact_header(
                    ui,
                    320.0,
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Core 状态").strong().small());
                            crate::widgets::badge(ui, core_label, core_tone);
                        });
                    },
                    |ui, rtl| {
                        if running {
                            for idx in if rtl { [1, 0] } else { [0, 1] } {
                                match idx {
                                    0 if crate::widgets::compact_button_enabled(
                                        ui,
                                        enabled,
                                        "重启 Core",
                                    )
                                    .clicked() =>
                                    {
                                        core_action = Some(Action::Restart);
                                    }
                                    1 if crate::widgets::compact_button_enabled(
                                        ui,
                                        stop_enabled,
                                        "停止 Core",
                                    )
                                    .clicked() =>
                                    {
                                        core_action = Some(Action::Stop);
                                    }
                                    _ => {}
                                }
                            }
                        } else if generation.is_some()
                            && crate::widgets::compact_button_enabled(ui, enabled, "启动 Core")
                                .clicked()
                        {
                            core_action = Some(Action::Restart);
                        }
                    },
                );
                if let Some(action) = core_action {
                    app.ui.dialog = Some(Dialog::Confirm(action));
                }

                let profile_desc = match generation.as_ref() {
                    Some(_) if running => name.clone(),
                    Some(_) => format!("{name}（已停止）"),
                    None => "尚未应用配置".into(),
                };
                let endpoint_str = if running {
                    endpoint.as_str()
                } else {
                    "Core 未运行"
                };
                let mut trigger_retest = false;
                let mut open_diag = false;
                let mut copy_ep_clicked = false;
                let can_copy_ep = running && endpoint.contains(':');
                let copy_ep = endpoint.replace("0.0.0.0:", "127.0.0.1:");
                crate::widgets::compact_subcard(ui, None, !running, |ui| {
                    let key_width = 104.0_f32.min(ui.available_width() * 0.34);
                    let kv_key = |ui: &mut egui::Ui, key: &str| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(key_width, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(key_width);
                                ui.set_max_width(key_width);
                                if !key.is_empty() {
                                    ui.add(
                                        egui::Label::new(RichText::new(key).small().weak())
                                            .truncate(),
                                    );
                                }
                            },
                        );
                    };
                    crate::widgets::kv_row_singleline(
                        ui,
                        "当前配置",
                        RichText::new(&profile_desc).strong(),
                        Some(&profile_desc),
                    );
                    crate::widgets::responsive_compact_header(
                        ui,
                        320.0,
                        |ui| {
                            ui.horizontal(|ui| {
                                kv_key(ui, "代理可用性");
                                let resp = ui.scope(|ui| {
                                    crate::widgets::badge(ui, &avail_badge, avail_tone);
                                });
                                if !avail_hover.is_empty() {
                                    resp.response.on_hover_text(&avail_hover);
                                }
                            });
                        },
                        |ui, rtl| {
                            let retest_label = if avail.is_some_and(|a| a.checking) {
                                "检测中…"
                            } else {
                                "重测"
                            };
                            for idx in if rtl { [1, 0] } else { [0, 1] } {
                                match idx {
                                    0 if crate::widgets::compact_button_enabled(
                                        ui,
                                        can_retest,
                                        retest_label,
                                    )
                                    .clicked() =>
                                    {
                                        trigger_retest = true;
                                    }
                                    1 if crate::widgets::compact_button(ui, "诊断详情")
                                        .clicked() =>
                                    {
                                        open_diag = true;
                                    }
                                    _ => {}
                                }
                            }
                        },
                    );
                    if !avail_hint.is_empty() {
                        ui.horizontal(|ui| {
                            kv_key(ui, "");
                            ui.add(
                                egui::Label::new(RichText::new(&avail_hint).small().weak())
                                    .truncate(),
                            )
                            .on_hover_text(&avail_hint);
                        });
                    }
                    crate::widgets::responsive_compact_header(
                        ui,
                        260.0,
                        |ui| {
                            ui.horizontal(|ui| {
                                kv_key(ui, "本机 HTTP 入口");
                                ui.add(
                                    egui::Label::new(RichText::new(endpoint_str).monospace())
                                        .truncate(),
                                )
                                .on_hover_text(endpoint_str);
                            });
                        },
                        |ui, _rtl| {
                            if can_copy_ep
                                && crate::widgets::compact_button(ui, "复制")
                                    .on_hover_text(&copy_ep)
                                    .clicked()
                            {
                                copy_ep_clicked = true;
                            }
                        },
                    );
                });
                if trigger_retest {
                    app.send(Command::CheckProxyAvailability { target_url: None });
                }
                if open_diag {
                    app.ui.page = Page::Diagnostics;
                }
                if copy_ep_clicked {
                    ui.ctx().copy_text(copy_ep.clone());
                    app.message = format!("已复制本地代理地址：{copy_ep}");
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                let (owned_label, owned_tone) = if !app.ui.connected {
                    ("状态待确认", crate::widgets::BadgeTone::Warn)
                } else if owned {
                    ("已接管", crate::widgets::BadgeTone::Success)
                } else if app
                    .status
                    .as_ref()
                    .is_some_and(|s| s.proxy_state == "Conflict")
                {
                    ("存在冲突", crate::widgets::BadgeTone::Warn)
                } else {
                    ("未接管", crate::widgets::BadgeTone::Muted)
                };
                let mut pending_mode: Option<ProxyMode> = None;
                crate::widgets::responsive_compact_header(
                    ui,
                    280.0,
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("系统代理").strong().small());
                            crate::widgets::badge(ui, owned_label, owned_tone);
                        });
                    },
                    |ui, rtl| {
                        let modes = [
                            (ProxyMode::Direct, "直连"),
                            (ProxyMode::Pac, "PAC"),
                            (ProxyMode::Global, "全局"),
                        ];
                        for idx in if rtl { [2, 1, 0] } else { [0, 1, 2] } {
                            let (mode, label) = modes[idx];
                            let selected = if owned {
                                committed == Some(mode)
                            } else {
                                mode == ProxyMode::Direct
                            };
                            if crate::widgets::compact_chip_enabled(
                                ui,
                                enabled && (mode == ProxyMode::Direct || running),
                                label,
                                selected,
                            )
                            .clicked()
                                && !selected
                            {
                                pending_mode = Some(mode);
                            }
                        }
                    },
                );
                if let Some(mode) = pending_mode {
                    app.ui.dialog = Some(Dialog::Confirm(Action::Proxy(mode)));
                }

                let (mode_desc, mode_hover) = match (running, owned, committed) {
                    (true, true, Some(ProxyMode::Pac)) => {
                        ("PAC 自动代理", "PAC 自动代理（由本程序接管）".to_owned())
                    }
                    (true, true, Some(ProxyMode::Global)) => {
                        ("全局代理", "全局代理（由本程序接管）".to_owned())
                    }
                    (true, false, _) => {
                        if app
                            .status
                            .as_ref()
                            .is_some_and(|s| s.proxy_state == "Conflict")
                            || !app.ui.connected
                        {
                            (proxy.as_str(), proxy.clone())
                        } else {
                            ("直连（未接管）", proxy.clone())
                        }
                    }
                    _ => (proxy.as_str(), proxy.clone()),
                };
                crate::widgets::compact_subcard(ui, None, false, |ui| {
                    crate::widgets::kv_row_singleline(
                        ui,
                        "系统代理方式",
                        RichText::new(mode_desc).strong(),
                        Some(&mode_hover),
                    );
                    if owned && let Some((pac_mode, pac_summary)) = app.active_pac_strategy() {
                        let (pac_short, pac_full) =
                            Self::pac_overview_summary(pac_mode, pac_summary.as_deref());
                        crate::widgets::kv_row_singleline(
                            ui,
                            "生效 PAC 策略",
                            RichText::new(&pac_short).strong(),
                            Some(&pac_full),
                        );
                    }
                });

                ui.add_space(6.0);
                let footer_note = if running || generation.is_some() {
                    "代理模式即时生效，配置修改需在「管理配置」中应用。"
                } else {
                    "请先「选择配置」并应用，启动 Core 后即可接管代理。"
                };
                crate::widgets::banner_box(ui, crate::widgets::BadgeTone::Accent, footer_note);
            });
        };
        let render_forwards_card = |app: &mut App, ui: &mut egui::Ui| {
            let ssh_ctl = app.ui.connected && !app.busy && app.ui.uncertain.is_none();
            card(ui, |ui| {
                let mut open_new_ssh = false;
                let mut open_manage = false;
                crate::widgets::responsive_header(
                    ui,
                    360.0,
                    |ui| {
                        ui.label(RichText::new("端口转发与隧道").strong().size(16.0));
                    },
                    |ui, rtl| {
                        for idx in if rtl { [1, 0] } else { [0, 1] } {
                            match idx {
                                0 if primary(ui, "+ SSH 隧道", ssh_ctl) => {
                                    open_new_ssh = true;
                                }
                                1 if ui.button("管理转发").clicked() => {
                                    open_manage = true;
                                }
                                _ => {}
                            }
                        }
                    },
                );
                if open_new_ssh {
                    let parsed = serde_json::from_str::<serde_json::Value>(&app.editor).ok();
                    let paused = app
                        .ui
                        .workspace
                        .paused_port_forwards
                        .get(&app.profile_id)
                        .cloned()
                        .unwrap_or_default();
                    let default_host = app.ui.ssh_state.hosts.first().map(|h| h.id.as_str());
                    app.ui.pf_form = crate::port_forward::PortForwardForm::from_new_ssh(
                        parsed.as_ref(),
                        &paused,
                        &app.ui.ssh_state.forwards,
                        default_host,
                    );
                    app.ui.pf_error = None;
                    app.ui.dialog = Some(Dialog::PortForward);
                }
                if open_manage {
                    app.ui.page = Page::Profiles;
                    app.ui.profile_tab = ProfileTab::PortForward;
                    app.ui.pf_subtab = PortForwardSubTab::Forwards;
                }
                ui.separator();
                crate::widgets::responsive_compact_header(
                    ui,
                    280.0,
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Xray 端口转发").strong().small());
                            if running && pf_count > 0 {
                                crate::widgets::badge(
                                    ui,
                                    &format!("{pf_count} 条生效中"),
                                    crate::widgets::BadgeTone::Success,
                                );
                            } else {
                                crate::widgets::badge(
                                    ui,
                                    "随 Core 运行",
                                    crate::widgets::BadgeTone::Muted,
                                );
                            }
                        });
                    },
                    |_, _| {},
                );
                if !running {
                    let msg = if generation.is_some() {
                        "Core 未运行，启动后自动恢复 Xray 端口转发。"
                    } else {
                        "尚未应用配置，应用后随 Core 启动转发。"
                    };
                    crate::widgets::compact_subcard(ui, None, true, |ui| {
                        ui.add(egui::Label::new(RichText::new(msg).small().weak()).truncate())
                            .on_hover_text(msg);
                    });
                } else if active_forwards.is_empty() {
                    let msg = "当前配置暂无已启用的 Xray 端口转发。";
                    crate::widgets::compact_subcard(ui, None, true, |ui| {
                        ui.add(egui::Label::new(RichText::new(msg).small().weak()).truncate())
                            .on_hover_text(msg);
                    });
                } else {
                    for entry in &active_forwards {
                        ui.push_id(("ov-xray-fwd", &entry.tag), |ui| {
                            let display = if entry.name.is_empty() {
                                entry.tag.as_str()
                            } else {
                                entry.name.as_str()
                            };
                            let net = crate::port_forward::network_label(&entry.network);
                            let full_title = format!("{display} ({net})");
                            let short_title = crate::widgets::ellipsize(&full_title, 28);
                            let copy_ip = if entry.listen == crate::port_forward::LISTEN_LAN {
                                crate::port_forward::LISTEN_LOCAL
                            } else {
                                entry.listen.as_str()
                            };
                            let copy_addr = format!("{copy_ip}:{}", entry.port);
                            crate::widgets::compact_subcard(ui, None, false, |ui| {
                                let mut copy_clicked = false;
                                crate::widgets::responsive_compact_header(
                                    ui,
                                    300.0,
                                    |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            let title_resp = ui.label(
                                                RichText::new(&short_title).strong().size(13.5),
                                            );
                                            if short_title != full_title {
                                                title_resp.on_hover_text(&full_title);
                                            }
                                            crate::widgets::badge(
                                                ui,
                                                "生效中",
                                                crate::widgets::BadgeTone::Success,
                                            );
                                        });
                                    },
                                    |ui, _rtl| {
                                        if crate::widgets::compact_button(ui, "复制")
                                            .on_hover_text(&copy_addr)
                                            .clicked()
                                        {
                                            copy_clicked = true;
                                        }
                                    },
                                );
                                if copy_clicked {
                                    ui.ctx().copy_text(copy_addr.clone());
                                    app.message = format!("已复制监听地址：{copy_addr}");
                                }
                                ui.horizontal(|ui| {
                                    let addr = format!(
                                        "{}:{} -> {}:{}",
                                        entry.listen,
                                        entry.port,
                                        entry.target_host,
                                        entry.target_port
                                    );
                                    let via = format!("· 经由 {}", entry.outbound_tag);
                                    let full = format!("{addr}  经由 {}", entry.outbound_tag);
                                    ui.add(
                                        egui::Label::new(RichText::new(addr).monospace().small())
                                            .truncate(),
                                    )
                                    .on_hover_text(&full);
                                    ui.add(
                                        egui::Label::new(RichText::new(via).small().weak())
                                            .truncate(),
                                    )
                                    .on_hover_text(&full);
                                });
                            });
                            ui.add_space(6.0);
                        });
                    }
                }
                let ssh_forwards = app.ui.ssh_state.forwards.clone();
                let ssh_hosts = app.ui.ssh_state.hosts.clone();
                let ssh_runtime = app.ui.ssh_state.runtime.clone();
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                let mut start_all = false;
                let mut stop_all = false;
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
                crate::widgets::responsive_compact_header(
                    ui,
                    320.0,
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("SSH 隧道").strong().small());
                            if !ssh_forwards.is_empty() && running_ssh_count > 0 {
                                crate::widgets::badge(
                                    ui,
                                    &format!("{running_ssh_count}/{} 运行中", ssh_forwards.len()),
                                    crate::widgets::BadgeTone::Success,
                                );
                            } else {
                                crate::widgets::badge(
                                    ui,
                                    "独立于 Core",
                                    crate::widgets::BadgeTone::Muted,
                                );
                            }
                        });
                    },
                    |ui, rtl| {
                        if ssh_forwards.len() >= 2 {
                            for idx in if rtl { [1, 0] } else { [0, 1] } {
                                match idx {
                                    0 if crate::widgets::compact_button_enabled(
                                        ui,
                                        ssh_ctl && !all_running,
                                        "全部启动",
                                    )
                                    .clicked() =>
                                    {
                                        start_all = true;
                                    }
                                    1 if crate::widgets::compact_button_enabled(
                                        ui,
                                        ssh_ctl && any_active,
                                        "全部停止",
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
                    app.send(Command::StartAllSshForwards);
                }
                if stop_all {
                    app.send(Command::StopAllSshForwards);
                }
                if ssh_forwards.is_empty() {
                    let msg = "暂无 SSH 隧道，点击右上角「+ SSH 隧道」新建。";
                    crate::widgets::compact_subcard(ui, None, true, |ui| {
                        ui.add(egui::Label::new(RichText::new(msg).small().weak()).truncate())
                            .on_hover_text(msg);
                    });
                } else {
                    for fwd in &ssh_forwards {
                        ui.push_id(("ov-ssh-fwd", &fwd.id), |ui| {
                            let rt = ssh_runtime.iter().find(|r| r.forward_id == fwd.id);
                            let state = rt.map(|r| r.state).unwrap_or(SshForwardState::Stopped);
                            let host_name = ssh_hosts
                                .iter()
                                .find(|h| h.id == fwd.ssh_host_id)
                                .map(|h| h.name.as_str())
                                .unwrap_or("未知跳板机");
                            let short_name = crate::widgets::ellipsize(&fwd.name, 28);
                            let copy_ip = if fwd.listen == crate::port_forward::LISTEN_LAN {
                                crate::port_forward::LISTEN_LOCAL
                            } else {
                                fwd.listen.as_str()
                            };
                            let copy_addr = format!("{copy_ip}:{}", fwd.port);
                            let lan_exposed = !crate::port_forward::is_loopback_listen(&fwd.listen);
                            let border_tone = (state == SshForwardState::Error)
                                .then_some(crate::widgets::BadgeTone::Danger);
                            crate::widgets::compact_subcard(
                                ui,
                                border_tone,
                                state == SshForwardState::Stopped,
                                |ui| {
                                    let mut copy_clicked = false;
                                    let mut toggle_clicked = false;
                                    let is_running = state == SshForwardState::Running;
                                    crate::widgets::responsive_compact_header(
                                        ui,
                                        300.0,
                                        |ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                let title_resp = ui.label(
                                                    RichText::new(&short_name).strong().size(13.5),
                                                );
                                                if short_name != fwd.name {
                                                    title_resp.on_hover_text(&fwd.name);
                                                }
                                                match state {
                                                    SshForwardState::Running => {
                                                        crate::widgets::badge(
                                                            ui,
                                                            "运行中",
                                                            crate::widgets::BadgeTone::Success,
                                                        )
                                                    }
                                                    SshForwardState::Connecting => {
                                                        crate::widgets::badge(
                                                            ui,
                                                            "连接中",
                                                            crate::widgets::BadgeTone::Accent,
                                                        )
                                                    }
                                                    SshForwardState::Error => {
                                                        crate::widgets::badge(
                                                            ui,
                                                            "失败",
                                                            crate::widgets::BadgeTone::Danger,
                                                        )
                                                    }
                                                    SshForwardState::Stopped => {
                                                        crate::widgets::badge(
                                                            ui,
                                                            "已停止",
                                                            crate::widgets::BadgeTone::Muted,
                                                        )
                                                    }
                                                }
                                                if lan_exposed {
                                                    crate::widgets::badge(
                                                        ui,
                                                        "局域网",
                                                        crate::widgets::BadgeTone::Warn,
                                                    );
                                                }
                                            });
                                        },
                                        |ui, rtl| {
                                            let action_label =
                                                if is_running { "停止" } else { "启动" };
                                            for idx in if rtl { [1, 0] } else { [0, 1] } {
                                                match idx {
                                                    0 if crate::widgets::compact_button(ui, "复制")
                                                        .on_hover_text(&copy_addr)
                                                        .clicked() =>
                                                    {
                                                        copy_clicked = true;
                                                    }
                                                    1 if crate::widgets::compact_button_enabled(
                                                        ui,
                                                        ssh_ctl,
                                                        action_label,
                                                    )
                                                    .clicked() =>
                                                    {
                                                        toggle_clicked = true;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        },
                                    );
                                    if copy_clicked {
                                        ui.ctx().copy_text(copy_addr.clone());
                                        app.message = format!("已复制监听地址：{copy_addr}");
                                    }
                                    if toggle_clicked {
                                        if is_running {
                                            app.send(Command::StopSshForward {
                                                id: fwd.id.clone(),
                                            });
                                        } else {
                                            app.send(Command::StartSshForward {
                                                id: fwd.id.clone(),
                                            });
                                        }
                                    }
                                    ui.horizontal(|ui| {
                                        let addr = format!(
                                            "{}:{} -> {}:{}",
                                            fwd.listen, fwd.port, fwd.target_host, fwd.target_port
                                        );
                                        let via = format!("· 经由 {host_name}");
                                        let full = format!("{addr}  经由 {host_name}");
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(addr).monospace().small(),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&full);
                                        ui.add(
                                            egui::Label::new(RichText::new(via).small().weak())
                                                .truncate(),
                                        )
                                        .on_hover_text(&full);
                                    });
                                    if let Some(err) = rt.and_then(|r| r.last_error.as_deref()) {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(err)
                                                    .small()
                                                    .color(ui.visuals().error_fg_color),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(err);
                                    }
                                },
                            );
                            ui.add_space(6.0);
                        });
                    }
                }
                ui.add_space(6.0);
                crate::widgets::banner_box(
                    ui,
                    crate::widgets::BadgeTone::Accent,
                    "直连模式下 Xray 转发仍可用，SSH 隧道可随时独立启停。",
                );
            });
        };
        if wide {
            let prev_spacing_x = ui.spacing().item_spacing.x;
            let gap = 14.0;
            let total = (ui.available_width() - gap).max(0.0);
            let left_w = (total * 0.48).clamp(400.0, 560.0).min(total * 0.5);
            let right_w = (total - left_w).max(0.0);
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = prev_spacing_x;
                            render_connection_card(self, ui);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(right_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = prev_spacing_x;
                            render_forwards_card(self, ui);
                        },
                    );
                });
            });
        } else {
            render_connection_card(self, ui);
            ui.add_space(12.0);
            render_forwards_card(self, ui);
        }
        if self.ui.workspace.overview_show_activity {
            ui.add_space(14.0);
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("最近活动与版本").strong().size(16.0));
                    if ui.button("查看日志").clicked() {
                        self.ui.page = Page::Logs;
                    }
                    if ui.button("隐藏").clicked() {
                        self.ui.workspace.overview_show_activity = false;
                        self.save_workspace();
                    }
                });
                if self.ui.events.is_empty() {
                    ui.label(RichText::new("暂无操作记录").weak());
                }
                for event in self.ui.events.iter().take(4) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(&event.message);
                        ui.label(
                            RichText::new(format!("{} 秒前", event.at.elapsed().as_secs()))
                                .small()
                                .weak(),
                        );
                    });
                }
                ui.separator();
                line(
                    ui,
                    "运行中的 Core",
                    if running {
                        generation
                            .as_ref()
                            .map(|g| g.candidate.core.version.as_str())
                            .unwrap_or("未知")
                    } else {
                        "未运行"
                    },
                );
                let next_core = if self.core_path.is_empty() {
                    "尚未选择".into()
                } else {
                    display_path_text(&self.core_path)
                };
                line(ui, "下次检查使用", &next_core);
            });
        }
    }
    fn pac_overview_summary(mode: PacMode, summary: Option<&str>) -> (String, String) {
        let short_mode = match mode {
            PacMode::AutoRules => "按路由自动生成",
            PacMode::DirectList => "指定直连",
            PacMode::ProxyList => "指定代理",
            PacMode::All => "全部代理",
            PacMode::Custom => "自定义脚本",
        };
        let full = match summary {
            Some(s) => format!("{}（{s}）", Self::pac_mode_label(mode)),
            None => Self::pac_mode_label(mode).into(),
        };
        let Some(raw) = summary.map(str::trim).filter(|s| !s.is_empty()) else {
            return (short_mode.to_owned(), full);
        };
        if let Some(count_seg) = raw
            .split(['（', '）', '(', ')'])
            .map(str::trim)
            .find(|seg| seg.contains("条生效"))
        {
            return (format!("{short_mode} · {count_seg}"), full);
        }
        let stripped = raw
            .trim_end_matches("，其余直连")
            .trim_end_matches("，其余代理")
            .trim_end_matches(", 其余直连")
            .trim_end_matches(", 其余代理")
            .trim();
        let short = if !stripped.is_empty() && stripped.chars().count() <= 16 {
            format!("{short_mode} · {stripped}")
        } else {
            short_mode.to_owned()
        };
        (short, full)
    }

    pub(in crate::app) fn proxy_label(&self) -> String {
        if !self.ui.connected {
            return "状态待确认".into();
        }
        match self.status.as_ref().map(|s| s.proxy_state.as_str()) {
            Some("Owned") => "由本程序接管".into(),
            Some("Unmanaged" | "Released") => "未由本程序接管".into(),
            Some("Conflict") => "代理设置存在外部冲突".into(),
            Some("RecoveryRequired" | "RestoreFailed") => "需要恢复处理".into(),
            Some(other) => format!("代理状态：{other}"),
            None => "尚未读取".into(),
        }
    }
}
