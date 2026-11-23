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
                        let status_text = if total_running_pf > 0 {
                            format!("Core 运行中 · {prof_part} · {total_running_pf} 条转发监听")
                        } else {
                            format!("Core 运行中 · {prof_part}")
                        };
                        crate::widgets::badge(ui, &status_text, crate::widgets::BadgeTone::Success);
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
                ui.label(RichText::new("连接与系统代理状态").strong().size(16.0));
                ui.add_space(4.0);
                let (core_label, core_tone) = if !app.ui.connected {
                    ("状态待确认", crate::widgets::BadgeTone::Warn)
                } else if running {
                    ("Core 运行中", crate::widgets::BadgeTone::Success)
                } else if app
                    .status
                    .as_ref()
                    .is_some_and(|s| s.core_state == "Degraded")
                {
                    ("Core 正在等待恢复", crate::widgets::BadgeTone::Warn)
                } else {
                    ("Core 已停止", crate::widgets::BadgeTone::Muted)
                };
                let (core_fg, _, _) = crate::widgets::tone_palette(ui, core_tone);
                crate::widgets::kv_row(
                    ui,
                    "Core 进程",
                    RichText::new(core_label).strong().color(core_fg),
                );
                ui.separator();
                let profile_desc = match generation.as_ref() {
                    Some(_) if running => name.clone(),
                    Some(_) => format!("{name}（已停止）"),
                    None => "尚未应用配置".into(),
                };
                crate::widgets::kv_row(ui, "当前配置", RichText::new(profile_desc).strong());
                ui.separator();
                let mode_desc = match (running, owned, committed) {
                    (true, true, Some(ProxyMode::Pac)) => "PAC 自动代理（由本程序接管）",
                    (true, true, Some(ProxyMode::Global)) => "全局代理（由本程序接管）",
                    _ => proxy.as_str(),
                };
                crate::widgets::kv_row(ui, "系统代理方式", RichText::new(mode_desc).strong());
                ui.separator();
                crate::widgets::kv_row(
                    ui,
                    "本机 HTTP 入口",
                    RichText::new(if running {
                        endpoint.as_str()
                    } else {
                        "Core 当前未运行"
                    })
                    .monospace(),
                );
                if owned && let Some((pac_mode, pac_summary)) = app.active_pac_strategy() {
                    ui.separator();
                    let desc = match pac_summary {
                        Some(summary) => {
                            format!("{}（{summary}）", Self::pac_mode_label(pac_mode))
                        }
                        None => Self::pac_mode_label(pac_mode).into(),
                    };
                    crate::widgets::kv_row(ui, "生效 PAC 策略", RichText::new(desc).strong());
                }
                ui.separator();
                let pf_summary = if running {
                    if pf_count > 0 {
                        format!("{pf_count} 条监听生效中")
                    } else {
                        "0 条监听".into()
                    }
                } else {
                    "已随 Core 停止".into()
                };
                crate::widgets::kv_row(ui, "Xray 端口转发", RichText::new(pf_summary).strong());
                ui.separator();
                let ssh_total = app.ui.ssh_state.forwards.len();
                let ssh_summary = if ssh_total == 0 {
                    RichText::new("0 条配置").weak()
                } else if running_ssh_count > 0 {
                    let (fg, _, _) =
                        crate::widgets::tone_palette(ui, crate::widgets::BadgeTone::Success);
                    RichText::new(format!("{running_ssh_count} / {ssh_total} 条运行中"))
                        .strong()
                        .color(fg)
                } else {
                    RichText::new(format!("0 / {ssh_total} 条运行中")).strong()
                };
                crate::widgets::kv_row(ui, "SSH 隧道", ssh_summary);
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("代理模式").small().weak());
                    for (mode, label) in [
                        (ProxyMode::Direct, "直连"),
                        (ProxyMode::Pac, "PAC"),
                        (ProxyMode::Global, "全局"),
                    ] {
                        let selected = if owned {
                            committed == Some(mode)
                        } else {
                            mode == ProxyMode::Direct
                        };
                        if crate::widgets::seg_button(
                            ui,
                            label,
                            selected,
                            enabled && (mode == ProxyMode::Direct || running),
                        )
                        .clicked()
                            && !selected
                        {
                            app.ui.dialog = Some(Dialog::Confirm(Action::Proxy(mode)));
                        }
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if running {
                        if ui
                            .add_enabled(
                                app.ui.connected && !app.busy && app.ui.uncertain.is_none(),
                                egui::Button::new("停止 Core"),
                            )
                            .clicked()
                        {
                            app.ui.dialog = Some(Dialog::Confirm(Action::Stop));
                        }
                        if ui
                            .add_enabled(enabled, egui::Button::new("重启 Core"))
                            .clicked()
                        {
                            app.ui.dialog = Some(Dialog::Confirm(Action::Restart));
                        }
                        if ui.button("管理配置").clicked() {
                            app.ui.page = Page::Profiles;
                        }
                    } else if generation.is_some() {
                        if primary(ui, "启动 Core", enabled) {
                            app.ui.dialog = Some(Dialog::Confirm(Action::Restart));
                        }
                        if ui.button("管理配置").clicked() {
                            app.ui.page = Page::Profiles;
                        }
                    } else if primary(ui, "选择配置", enabled) {
                        app.ui.page = Page::Profiles;
                    }
                    if ui.button("路由诊断").clicked() {
                        app.ui.page = Page::Diagnostics;
                    }
                });
            });
        };
        let render_forwards_card = |app: &mut App, ui: &mut egui::Ui| {
            let ssh_ctl = app.ui.connected && !app.busy && app.ui.uncertain.is_none();
            card(ui, |ui| {
                let mut open_new_ssh = false;
                let mut open_manage = false;
                crate::widgets::responsive_header(
                    ui,
                    380.0,
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
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Xray 端口转发").strong().small());
                    if running && pf_count > 0 {
                        crate::widgets::badge(
                            ui,
                            &format!("{pf_count} 条生效中"),
                            crate::widgets::BadgeTone::Success,
                        );
                    } else {
                        crate::widgets::badge(ui, "随 Core 运行", crate::widgets::BadgeTone::Muted);
                    }
                });
                if !running {
                    let msg = if generation.is_some() {
                        "Core 当前未运行。点击左侧「启动 Core」后将自动恢复 Xray 端口转发监听。"
                    } else {
                        "尚未应用配置。点击左侧「选择配置」并应用后，已启用的 Xray 端口转发将随 Core 启动。"
                    };
                    ui.add(egui::Label::new(RichText::new(msg).small().weak()).wrap());
                } else if active_forwards.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new("当前运行配置暂无已启用的 Xray 端口转发。")
                                .small()
                                .weak(),
                        )
                        .wrap(),
                    );
                } else {
                    for entry in &active_forwards {
                        ui.push_id(("ov-xray-fwd", &entry.tag), |ui| {
                            let display = if entry.name.is_empty() {
                                entry.tag.as_str()
                            } else {
                                entry.name.as_str()
                            };
                            let net = crate::port_forward::network_label(&entry.network);
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
                                            ui.label(
                                                RichText::new(format!("{display} ({net})"))
                                                    .strong()
                                                    .size(13.5),
                                            );
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
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{}:{} -> {}:{}",
                                            entry.listen,
                                            entry.port,
                                            entry.target_host,
                                            entry.target_port
                                        ))
                                        .monospace()
                                        .small(),
                                    );
                                    ui.label(
                                        RichText::new(format!("经由 {}", entry.outbound_tag))
                                            .small()
                                            .weak(),
                                    );
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
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "暂无 SSH 隧道；点击上方「+ SSH 隧道」可快速新建并按需启停。",
                            )
                            .small()
                            .weak(),
                        )
                        .wrap(),
                    );
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
                                                ui.label(
                                                    RichText::new(&fwd.name).strong().size(13.5),
                                                );
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
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "{}:{} -> {}:{}",
                                                fwd.listen,
                                                fwd.port,
                                                fwd.target_host,
                                                fwd.target_port
                                            ))
                                            .monospace()
                                            .small(),
                                        );
                                        ui.label(
                                            RichText::new(format!("经由 {host_name}"))
                                                .small()
                                                .weak(),
                                        );
                                    });
                                    if let Some(err) = rt.and_then(|r| r.last_error.as_deref()) {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(err)
                                                    .small()
                                                    .color(ui.visuals().error_fg_color),
                                            )
                                            .wrap(),
                                        );
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
                    "说明：即使将系统代理切回「直连」，只要 Core 仍在运行，Xray 端口转发保持可用；SSH 隧道转发则可随时独立按需启停。",
                );
            });
        };
        if wide {
            let prev_spacing_x = ui.spacing().item_spacing.x;
            let gap = 14.0;
            let total = (ui.available_width() - gap).max(0.0);
            let left_w = (total * 0.46).clamp(400.0, 560.0).min(total * 0.5);
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
