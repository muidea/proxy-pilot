use super::*;

fn step_tone(status: DiagnosticStepStatus) -> crate::widgets::BadgeTone {
    match status {
        DiagnosticStepStatus::Pass => crate::widgets::BadgeTone::Success,
        DiagnosticStepStatus::Warn => crate::widgets::BadgeTone::Warn,
        DiagnosticStepStatus::Fail => crate::widgets::BadgeTone::Danger,
        DiagnosticStepStatus::Checking => crate::widgets::BadgeTone::Accent,
        DiagnosticStepStatus::Skipped => crate::widgets::BadgeTone::Muted,
    }
}

fn availability_state_label(state: ProxyAvailabilityState) -> &'static str {
    match state {
        ProxyAvailabilityState::Stopped => "未运行",
        ProxyAvailabilityState::Untested => "待检测",
        ProxyAvailabilityState::Checking => "检测中",
        ProxyAvailabilityState::Available => "可用",
        ProxyAvailabilityState::Degraded => "网络波动",
        ProxyAvailabilityState::DirectOnly => "直连可达（未走代理）",
        ProxyAvailabilityState::Unavailable => "不可用",
        ProxyAvailabilityState::Unsupported => "不支持",
    }
}

fn circled_step_index(index: u8) -> String {
    match index {
        1 => "①".into(),
        2 => "②".into(),
        3 => "③".into(),
        4 => "④".into(),
        5 => "⑤".into(),
        _ => index.to_string(),
    }
}

fn redact_probe_url(url: &str) -> String {
    let trimmed = url.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return trimmed.into();
    };
    let (authority, suffix) = match rest.find(['/', '?', '#']) {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    };
    let redacted_authority = match authority.rsplit_once('@') {
        Some((_, host_port)) => format!("***@{host_port}"),
        None => authority.to_owned(),
    };
    format!("{scheme}://{redacted_authority}{suffix}")
}

impl App {
    pub(in crate::app) fn format_proxy_diagnostic_report(&self) -> String {
        let mut out = String::from("# Proxy Pilot 代理可用性诊断报告\n\n");
        let conn = if self.ui.connected {
            "已连接"
        } else {
            "未连接（以下为最后已知状态）"
        };
        let core_state = self
            .status
            .as_ref()
            .map(|s| s.core_state.as_str())
            .unwrap_or("未读取");
        let profile = self
            .status
            .as_ref()
            .and_then(|s| s.generation.as_ref())
            .map(|g| self.ui.workspace.name(&g.candidate.profile_id).to_string())
            .unwrap_or_else(|| "未应用配置".into());
        let mode = self
            .status
            .as_ref()
            .and_then(|s| s.generation.as_ref())
            .map(|g| mode_name(g.proxy_mode))
            .unwrap_or("未应用");
        let gen_id = self
            .status
            .as_ref()
            .and_then(|s| s.generation.as_ref())
            .map(|g| g.id.as_str())
            .unwrap_or("-");
        let inst_id = self
            .status
            .as_ref()
            .and_then(|s| s.runtime_instance_id.as_deref())
            .unwrap_or("-");
        out.push_str(&format!("- 后台连接: {conn}\n"));
        out.push_str(&format!("- Core 状态: {core_state}\n"));
        out.push_str(&format!(
            "- 当前配置: {profile} · 代理模式: {mode} · 系统代理: {}\n",
            self.proxy_label()
        ));
        out.push_str(&format!("- 生效代次: {gen_id} · 运行实例: {inst_id}\n\n"));

        let Some(avail) = self
            .status
            .as_ref()
            .and_then(|s| s.proxy_availability.as_ref())
        else {
            out.push_str("## 诊断结论\n\n- 状态: 暂无探测结果\n");
            return out;
        };

        out.push_str("## 诊断结论\n\n");
        out.push_str(&format!(
            "- 状态: {} ({})\n",
            availability_state_label(avail.state),
            if avail.badge_text.is_empty() {
                "-"
            } else {
                &avail.badge_text
            }
        ));
        if !avail.headline.is_empty() {
            out.push_str(&format!("- 结论: {}\n", avail.headline));
        }
        if !avail.target_url.is_empty() {
            out.push_str(&format!(
                "- 探测目标: {}\n",
                redact_probe_url(&avail.target_url)
            ));
        }
        if let Some(checked_at) = avail.checked_at.as_deref() {
            out.push_str(&format!("- 检测时间: {checked_at}\n"));
        }
        if let Some(in_tag) = avail.inbound_tag.as_deref() {
            let proto = avail.inbound_protocol.as_deref().unwrap_or("-");
            out.push_str(&format!("- 本地入站: {in_tag} ({proto})\n"));
        }
        if let Some(expected_out) = avail.expected_outbound.as_deref() {
            out.push_str(&format!("- 静态路由预测出站: {expected_out}\n"));
        }
        out.push_str(&format!(
            "- 耗时与状态码: 本地={}ms, TLS={}ms, 总计={}ms, HTTP={}\n",
            avail
                .local_ms
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            avail
                .tls_ms
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            avail
                .total_ms
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            avail
                .http_status
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
        ));
        if let Some(stage) = avail.fail_stage.as_deref() {
            out.push_str(&format!("- 失败阶段: {stage}\n"));
        }
        if let Some(detail) = avail.fail_detail.as_deref() {
            out.push_str(&format!("- 失败详情: {detail}\n"));
        }
        if !avail.suggestions.is_empty() {
            out.push_str("- 修复建议:\n");
            for sug in &avail.suggestions {
                out.push_str(&format!("  - {sug}\n"));
            }
        }

        if !avail.steps.is_empty() {
            out.push_str("\n## 五步链路核查\n\n");
            for step in &avail.steps {
                let ev = step
                    .evidence
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- [{}] {}: {} — {}{ev}\n",
                    step.index, step.name, step.label, step.detail
                ));
            }
        }

        if !avail.observatory.is_empty() {
            out.push_str("\n## Xray Native Observatory（只读）\n\n");
            for obs in &avail.observatory {
                let delay = obs
                    .delay_ms
                    .map(|ms| format!("{ms}ms"))
                    .unwrap_or_else(|| "-".into());
                let err = obs.last_error.as_deref().unwrap_or("-");
                out.push_str(&format!(
                    "- {}: alive={}, delay={delay}, error={err}\n",
                    obs.outbound_tag, obs.alive
                ));
            }
        }

        if !avail.history.is_empty() {
            out.push_str("\n## 近期探测样本（最近 5 条）\n\n");
            for sample in avail.history.iter().rev().take(5) {
                let tot = sample
                    .total_ms
                    .map(|ms| format!("{ms}ms"))
                    .unwrap_or_else(|| "-".into());
                let tls = sample
                    .tls_ms
                    .map(|ms| format!("{ms}ms"))
                    .unwrap_or_else(|| "-".into());
                let code = sample
                    .http_status
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".into());
                out.push_str(&format!(
                    "- {} | {} | total={tot} | TLS {tls} | HTTP {code} | {}\n",
                    sample.timestamp,
                    availability_state_label(sample.state),
                    sample.note
                ));
            }
        }
        out
    }

    fn proxy_availability_report_card(&mut self, ui: &mut egui::Ui) {
        let avail = self
            .status
            .as_ref()
            .and_then(|s| s.proxy_availability.as_ref())
            .cloned();
        let running = self.is_running();
        let checking = avail.as_ref().is_some_and(|a| a.checking);
        let unsupported = avail
            .as_ref()
            .is_some_and(|a| a.state == ProxyAvailabilityState::Unsupported);
        let can_probe =
            running && self.ui.connected && !self.shutdown_requested && !checking && !unsupported;

        let mut trigger_default = false;
        let mut trigger_adhoc = false;
        let mut copy_report = false;

        card(ui, |ui| {
            crate::widgets::responsive_header(
                ui,
                460.0,
                |ui| {
                    ui.label(RichText::new("代理可用性诊断报告").strong().size(18.0));
                },
                |ui, rtl| {
                    let retest_label = if checking {
                        "诊断中…"
                    } else {
                        "立即重新诊断"
                    };
                    for idx in if rtl { [1, 0] } else { [0, 1] } {
                        match idx {
                            0 => {
                                if primary(ui, retest_label, can_probe) {
                                    trigger_default = true;
                                }
                            }
                            _ => {
                                if ui.button("复制诊断报告").clicked() {
                                    copy_report = true;
                                }
                            }
                        }
                    }
                },
            );
            ui.add_space(6.0);

            match &avail {
                Some(a) => {
                    let tone = if a.checking {
                        crate::widgets::BadgeTone::Accent
                    } else {
                        availability_tone(a.state)
                    };
                    let badge_label = if a.checking {
                        "检测中…"
                    } else if a.badge_text.is_empty() {
                        availability_state_label(a.state)
                    } else {
                        a.badge_text.as_str()
                    };
                    let headline = if a.headline.trim().is_empty() {
                        availability_state_label(a.state)
                    } else {
                        a.headline.as_str()
                    };
                    crate::widgets::subcard(ui, Some(tone), false, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            crate::widgets::badge(ui, badge_label, tone);
                            ui.add(egui::Label::new(RichText::new(headline).strong()).wrap());
                            if let Some(gen_id) = a.generation_id.as_deref() {
                                let short_gen: String = gen_id.chars().take(16).collect();
                                crate::widgets::mono_badge(
                                    ui,
                                    &short_gen,
                                    crate::widgets::BadgeTone::Muted,
                                );
                            }
                            if a.tray_throttled {
                                crate::widgets::badge(
                                    ui,
                                    "托盘降频中",
                                    crate::widgets::BadgeTone::Muted,
                                );
                            }
                        });

                        let mut meta_parts = Vec::new();
                        if !a.target_url.is_empty() {
                            meta_parts.push(format!("目标 {}", redact_probe_url(&a.target_url)));
                        }
                        if let Some(ts) = a.checked_at.as_deref() {
                            meta_parts.push(format!("时间 {ts}"));
                        }
                        if let (Some(loc), Some(tls), Some(tot)) =
                            (a.local_ms, a.tls_ms, a.total_ms)
                        {
                            meta_parts
                                .push(format!("本地 {loc} ms · TLS {tls} ms · 总计 {tot} ms"));
                        } else if let Some(tot) = a.total_ms {
                            meta_parts.push(format!("总计 {tot} ms"));
                        }
                        if let Some(code) = a.http_status {
                            meta_parts.push(format!("HTTP {code}"));
                        }
                        if let Some(in_tag) = a.inbound_tag.as_deref() {
                            let route_out = a.expected_outbound.as_deref().unwrap_or("未确定");
                            meta_parts.push(format!("链路 {in_tag} → {route_out}"));
                        }
                        if !meta_parts.is_empty() {
                            ui.add_space(4.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(meta_parts.join(" · ")).small().weak(),
                                )
                                .wrap(),
                            );
                        }

                        if let Some(stage) = a.fail_stage.as_deref().filter(|s| !s.is_empty()) {
                            ui.add_space(4.0);
                            let (danger_fg, _, _) =
                                crate::widgets::tone_palette(ui, crate::widgets::BadgeTone::Danger);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("卡点阶段：{stage}"))
                                        .small()
                                        .strong()
                                        .color(danger_fg),
                                )
                                .wrap(),
                            );
                        }
                        if let Some(detail) = a.fail_detail.as_deref().filter(|s| !s.is_empty()) {
                            ui.add(egui::Label::new(RichText::new(detail).small().weak()).wrap());
                        }
                        for sug in &a.suggestions {
                            ui.add(
                                egui::Label::new(RichText::new(format!("• {sug}")).small()).wrap(),
                            );
                        }
                    });
                }
                None => {
                    crate::widgets::subcard(
                        ui,
                        Some(crate::widgets::BadgeTone::Muted),
                        false,
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                crate::widgets::badge(
                                    ui,
                                    if running { "待检测" } else { "Core 未运行" },
                                    crate::widgets::BadgeTone::Muted,
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(if running {
                                            "尚未生成本轮代理诊断结果，可点击「立即重新诊断」或指定地址测速。"
                                        } else {
                                            "Core 启动后将自动执行首轮端到端代理诊断；你也可以在设置中调整探测策略。"
                                        })
                                        .small()
                                        .weak(),
                                    )
                                    .wrap(),
                                );
                            });
                        },
                    );
                }
            }

            ui.add_space(8.0);
            ui.add(
                egui::Label::new(
                    RichText::new("按指定地址诊断（仅本次单次探测，不修改默认设置，支持 http:// 与 https://）")
                        .small()
                        .weak(),
                )
                .wrap(),
            );
            ui.horizontal_wrapped(|ui| {
                let url_w = (ui.available_width() - 120.0).clamp(160.0, 420.0);
                crate::widgets::singleline_select_all(
                    ui,
                    &mut self.ui.probe_adhoc_url,
                    url_w,
                    proxy_model::DEFAULT_PROXY_PROBE_URL,
                );
                if ui
                    .add_enabled(
                        can_probe && !self.ui.probe_adhoc_url.trim().is_empty(),
                        egui::Button::new("测试此地址"),
                    )
                    .clicked()
                {
                    trigger_adhoc = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("常用预设：").small().weak());
                for (preset, title) in [
                    (
                        "https://cp.cloudflare.com/generate_204",
                        "Cloudflare HTTPS 204",
                    ),
                    (
                        "http://cp.cloudflare.com/generate_204",
                        "Cloudflare HTTP 204",
                    ),
                    ("https://www.gstatic.com/generate_204", "gstatic HTTPS 204"),
                ] {
                    if crate::widgets::compact_chip(
                        ui,
                        title,
                        self.ui.probe_adhoc_url.trim() == preset,
                    )
                    .clicked()
                    {
                        self.ui.probe_adhoc_url = preset.to_owned();
                    }
                }
            });

            if let Some(a) = &avail {
                if !a.steps.is_empty() {
                    ui.add_space(10.0);
                    ui.label(RichText::new("五步链路核查").strong());
                    ui.add_space(4.0);
                    for step in &a.steps {
                        let tone = step_tone(step.status);
                        let dimmed = step.status == DiagnosticStepStatus::Skipped;
                        crate::widgets::compact_subcard(ui, Some(tone), dimmed, |ui| {
                            crate::widgets::responsive_compact_header(
                                ui,
                                320.0,
                                |ui| {
                                    ui.horizontal(|ui| {
                                        crate::widgets::mono_badge(
                                            ui,
                                            &circled_step_index(step.index),
                                            crate::widgets::BadgeTone::Muted,
                                        );
                                        ui.add(
                                            egui::Label::new(RichText::new(&step.name).strong())
                                                .wrap(),
                                        );
                                    });
                                },
                                |ui, _| {
                                    crate::widgets::badge(ui, &step.label, tone);
                                },
                            );
                            if !step.detail.is_empty() {
                                ui.add(
                                    egui::Label::new(RichText::new(&step.detail).small()).wrap(),
                                );
                            }
                            if let Some(ev) = step.evidence.as_deref().filter(|s| !s.is_empty()) {
                                ui.add(
                                    egui::Label::new(RichText::new(ev).small().monospace().weak())
                                        .wrap(),
                                );
                            }
                        });
                        ui.add_space(4.0);
                    }
                }

                if !a.observatory.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Xray 原生出站状态（只读 Observatory）").strong());
                    ui.add_space(4.0);
                    for obs in &a.observatory {
                        let obs_tone = if obs.alive {
                            crate::widgets::BadgeTone::Success
                        } else {
                            crate::widgets::BadgeTone::Danger
                        };
                        crate::widgets::compact_subcard(ui, Some(obs_tone), false, |ui| {
                            crate::widgets::responsive_compact_header(
                                ui,
                                280.0,
                                |ui| {
                                    ui.horizontal(|ui| {
                                        crate::widgets::mono_badge(
                                            ui,
                                            &obs.outbound_tag,
                                            crate::widgets::BadgeTone::Muted,
                                        );
                                        let note = match (obs.delay_ms, obs.last_error.as_deref()) {
                                            (Some(ms), Some(err)) => format!("{ms} ms · {err}"),
                                            (Some(ms), None) => format!("{ms} ms"),
                                            (None, Some(err)) => err.to_owned(),
                                            (None, None) => "无附加指标".into(),
                                        };
                                        ui.add(
                                            egui::Label::new(RichText::new(note).small().weak())
                                                .wrap(),
                                        );
                                    });
                                },
                                |ui, _| {
                                    crate::widgets::badge(
                                        ui,
                                        if obs.alive { "存活" } else { "离线" },
                                        obs_tone,
                                    );
                                },
                            );
                        });
                        ui.add_space(4.0);
                    }
                }

                if !a.history.is_empty() {
                    ui.add_space(8.0);
                    let total = a.history.len();
                    let pass_count = a
                        .history
                        .iter()
                        .filter(|s| s.state == ProxyAvailabilityState::Available)
                        .count();
                    let pass_rate_pct = (pass_count * 100) / total;
                    let ok_latencies: Vec<u64> = a
                        .history
                        .iter()
                        .filter(|s| s.state == ProxyAvailabilityState::Available)
                        .filter_map(|s| s.total_ms)
                        .collect();
                    let avg_ms = (!ok_latencies.is_empty())
                        .then(|| ok_latencies.iter().sum::<u64>() / ok_latencies.len() as u64);
                    let max_ms = a
                        .history
                        .iter()
                        .filter_map(|s| s.total_ms)
                        .max()
                        .unwrap_or(0);

                    ui.label(RichText::new("近期稳定性与延迟曲线（最近 10 次）").strong());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "通过率 {pass_rate_pct}%（{pass_count}/{total}）"
                            ))
                            .small(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "成功均值 {}",
                                avg_ms
                                    .map(|v| format!("{v} ms"))
                                    .unwrap_or_else(|| "--".into())
                            ))
                            .small()
                            .weak(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "峰值耗时 {}",
                                if max_ms > 0 {
                                    format!("{max_ms} ms")
                                } else {
                                    "--".into()
                                }
                            ))
                            .small()
                            .weak(),
                        );
                    });
                    let bar_area_w = ui.available_width().clamp(160.0, 440.0);
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(bar_area_w, 34.0), egui::Sense::hover());
                    if ui.is_rect_visible(rect) {
                        let slots = 10usize;
                        let gap = 4.0_f32;
                        let bar_w =
                            ((rect.width() - gap * (slots as f32 - 1.0)) / slots as f32).max(6.0);
                        let max_scale = max_ms.max(200) as f32;
                        let empty_fill = crate::widgets::panel_alt_color(ui);
                        let offset = slots.saturating_sub(a.history.len());
                        for idx in 0..slots {
                            let x0 = rect.min.x + idx as f32 * (bar_w + gap);
                            let x1 = (x0 + bar_w).min(rect.max.x);
                            if x1 <= x0 {
                                continue;
                            }
                            if idx < offset {
                                let bar_rect = egui::Rect::from_min_max(
                                    egui::pos2(x0, rect.max.y - 4.0),
                                    egui::pos2(x1, rect.max.y),
                                );
                                ui.painter().rect_filled(bar_rect, 2.0, empty_fill);
                            } else if let Some(sample) = a.history.get(idx - offset) {
                                let (fg, _, _) = crate::widgets::tone_palette(
                                    ui,
                                    availability_tone(sample.state),
                                );
                                let frac = sample
                                    .total_ms
                                    .map(|ms| (ms as f32 / max_scale).clamp(0.22, 1.0))
                                    .unwrap_or(0.30);
                                let bar_h = (rect.height() * frac).clamp(6.0, rect.height());
                                let bar_rect = egui::Rect::from_min_max(
                                    egui::pos2(x0, rect.max.y - bar_h),
                                    egui::pos2(x1, rect.max.y),
                                );
                                ui.painter().rect_filled(bar_rect, 3.0, fg);
                            }
                        }
                    }
                    resp.on_hover_text(format!(
                        "最近 {} 次探测（由左至右由旧到新）：绿色=可用，橙色=波动/直连，红色=不可用",
                        a.history.len()
                    ));

                    egui::CollapsingHeader::new(format!(
                        "查看近期探测明细（最近 {} 条）",
                        a.history.len()
                    ))
                    .show(ui, |ui| {
                        for sample in a.history.iter().rev() {
                            ui.horizontal_wrapped(|ui| {
                                crate::widgets::badge(
                                    ui,
                                    availability_state_label(sample.state),
                                    availability_tone(sample.state),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&sample.timestamp).small().monospace().weak(),
                                    )
                                    .wrap(),
                                );
                                let lat = match (sample.total_ms, sample.tls_ms, sample.http_status)
                                {
                                    (Some(tot), Some(tls), Some(code)) => {
                                        format!("{tot} ms (TLS {tls} ms) · HTTP {code}")
                                    }
                                    (Some(tot), None, Some(code)) => {
                                        format!("{tot} ms · HTTP {code}")
                                    }
                                    (Some(tot), _, None) => format!("{tot} ms"),
                                    _ => "-".into(),
                                };
                                ui.add(egui::Label::new(RichText::new(lat).small()).wrap());
                                if !sample.note.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&sample.note).small().weak(),
                                        )
                                        .wrap(),
                                    );
                                }
                            });
                        }
                    });
                }
            }
        });

        if trigger_default {
            self.send(Command::CheckProxyAvailability { target_url: None });
        } else if trigger_adhoc {
            let target = self.ui.probe_adhoc_url.trim().to_owned();
            if !target.is_empty() {
                self.send(Command::CheckProxyAvailability {
                    target_url: Some(target),
                });
            }
        }
        if copy_report {
            let report = self.format_proxy_diagnostic_report();
            ui.ctx().copy_text(report);
            self.message = "已复制诊断报告（已脱敏）。".into();
        }
    }

    pub(in crate::app) fn diagnostics(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "诊断",
            "进程、代理归属与本机探针分别呈现；路由只做静态预测，实际出口需外部验证。",
        );
        self.proxy_availability_report_card(ui);
        ui.add_space(16.0);
        card(ui, |ui| {
            ui.label(RichText::new("本机状态").strong().size(18.0));
            line(
                ui,
                "后台连接",
                if self.ui.connected {
                    "已连接"
                } else {
                    "中断 · 下列为最后已知状态"
                },
            );
            line(
                ui,
                "Core 进程",
                self.status
                    .as_ref()
                    .map(|s| match s.core_state.as_str() {
                        "Running" => "运行中",
                        "Degraded" => "降级 / 等待恢复",
                        _ => "已停止",
                    })
                    .unwrap_or("未读取"),
            );
            line(ui, "系统代理归属", &self.proxy_label());
            if let Some(status) = &self.status {
                line(
                    ui,
                    "代理模式",
                    status
                        .generation
                        .as_ref()
                        .map(|g| mode_name(g.proxy_mode))
                        .unwrap_or("未应用"),
                );
                if let Some((pac_mode, pac_summary)) = self.active_pac_strategy() {
                    let desc = match pac_summary {
                        Some(summary) => {
                            format!("{}（{summary}）", Self::pac_mode_label(pac_mode))
                        }
                        None => Self::pac_mode_label(pac_mode).into(),
                    };
                    line(ui, "生效 PAC 策略", &desc);
                }
                line(
                    ui,
                    "恢复状态",
                    if status.recovery_required {
                        "需要安全恢复"
                    } else {
                        "正常"
                    },
                );
                if let Some(notice) = &status.auto_connect_notice {
                    line(ui, "自动连接", notice);
                }
                if let Some(error) = &status.last_error {
                    line(ui, "最近错误", &error.code);
                    egui::CollapsingHeader::new("错误详情").show(ui, |ui| {
                        ui.add(egui::Label::new(&error.message).wrap());
                    });
                }
            }
            if self.can_resolve_proxy_conflict()
                && ui.button("保留当前系统代理并解除冲突…").clicked()
            {
                self.ui.dialog = Some(Dialog::Confirm(Action::ResolveProxyConflict));
            }
            line(
                ui,
                "本地入口",
                match self.status.as_ref().map(|s| s.listener_state.as_str()) {
                    Some("Ready") => "就绪（本机探针已通过）",
                    Some("Starting") => "启动或恢复中",
                    Some(_) => "未运行",
                    None => "未读取",
                },
            );
            line(
                ui,
                "PAC 服务",
                match self.status.as_ref().map(|s| s.pac_state.as_str()) {
                    Some("Serving") => "已发布",
                    Some(_) => "未发布",
                    None => "未读取",
                },
            );
            if let Some(winhttp) = self.status.as_ref().and_then(|s| s.winhttp.as_ref()) {
                ui.label(
                    RichText::new(
                        "用途：查看 WinHTTP 客户端（独立于 WinINet 系统代理）的默认会话代理来源；只读，不影响本程序的代理接管。",
                    )
                    .small()
                    .weak(),
                );
                let summary = match winhttp.access_type.as_str() {
                    "NAMED_PROXY" => "显式代理",
                    "NO_PROXY" => "不使用代理",
                    _ => "默认 / 系统代理",
                };
                line(ui, "WinHTTP 会话代理", summary);
                egui::CollapsingHeader::new("WinHTTP 只读诊断").show(ui, |ui| {
                    line(ui, "访问类型", &winhttp.access_type);
                    line(
                        ui,
                        "代理",
                        if winhttp.proxy.is_empty() {
                            "（空）"
                        } else {
                            &winhttp.proxy
                        },
                    );
                    line(
                        ui,
                        "绕过",
                        if winhttp.bypass.is_empty() {
                            "（空）"
                        } else {
                            &winhttp.bypass
                        },
                    );
                    ui.label(
                        RichText::new(
                            "由 WinHTTP 默认会话只读获取；本程序不修改机器级或 WinHTTP 代理。",
                        )
                        .small()
                        .weak(),
                    );
                });
            }
            if ui.button("重新读取状态").clicked() {
                self.send(Command::GetStatus);
            }
            if !self.ui.operation_details.is_empty() {
                egui::CollapsingHeader::new("最近查询的操作证据").show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("operation-details")
                        .max_height(240.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&self.ui.operation_details).monospace(),
                                )
                                .wrap(),
                            );
                        });
                });
            }
            if let Some(request_id) = self.failed_request.clone()
                && ui.button("查询最近失败请求").clicked()
            {
                self.send(Command::GetOperation { request_id });
            }
        });
        ui.add_space(16.0);
        let runtime_inbounds: Vec<serde_json::Value> = self
            .status
            .as_ref()
            .and_then(|s| s.generation.as_ref())
            .and_then(|g| {
                self.ui
                    .runtime
                    .as_ref()
                    .filter(|r| r.generation_id == g.id)
                    .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.json).ok())
            })
            .and_then(|v| v.get("inbounds")?.as_array().cloned())
            .unwrap_or_default();
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("本地入口与端口转发就绪状态")
                        .strong()
                        .size(18.0),
                );
                crate::widgets::badge(
                    ui,
                    &format!("{} 项", runtime_inbounds.len()),
                    if runtime_inbounds.is_empty() {
                        crate::widgets::BadgeTone::Muted
                    } else {
                        crate::widgets::BadgeTone::Accent
                    },
                );
            });
            if runtime_inbounds.is_empty() {
                ui.label(
                    RichText::new("当前没有已发布的运行入站；启动 Core 后将显示每个入口及端口转发的就绪探针状态。")
                        .small()
                        .weak(),
                );
            } else {
                let running_core = self.is_running();
                let ready = self
                    .status
                    .as_ref()
                    .is_some_and(|s| s.listener_state == "Ready");
                for ib in &runtime_inbounds {
                    let tag = ib.get("tag").and_then(|v| v.as_str()).unwrap_or("未命名");
                    let proto = ib
                        .get("protocol")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let listen = ib
                        .get("listen")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0.0.0.0");
                    let port = ib.get("port").and_then(|v| v.as_u64()).unwrap_or(0);
                    let (detail, status, tone) = if proto == "dokodemo-door" {
                        let net = crate::port_forward::normalize_network(
                            ib.pointer("/settings/network").and_then(|v| v.as_str()),
                        );
                        let t_host = ib
                            .pointer("/settings/address")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let t_port = ib
                            .pointer("/settings/port")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let (status, tone) = if !running_core {
                            ("Core 未运行", crate::widgets::BadgeTone::Muted)
                        } else if net == "udp" {
                            (
                                "UDP 转发已校验（不做 TCP 探针）",
                                crate::widgets::BadgeTone::Accent,
                            )
                        } else if ready {
                            ("TCP 监听就绪", crate::widgets::BadgeTone::Success)
                        } else {
                            ("启动中", crate::widgets::BadgeTone::Warn)
                        };
                        (
                            format!("转发 · {listen}:{port} -> {t_host}:{t_port}"),
                            status,
                            tone,
                        )
                    } else {
                        let (status, tone) = if !running_core {
                            ("Core 未运行", crate::widgets::BadgeTone::Muted)
                        } else if ready && matches!(proto, "http" | "socks" | "mixed") {
                            ("协议握手就绪", crate::widgets::BadgeTone::Success)
                        } else if ready {
                            ("Core 已校验", crate::widgets::BadgeTone::Accent)
                        } else {
                            ("启动中", crate::widgets::BadgeTone::Warn)
                        };
                        (format!("{proto} · {listen}:{port}"), status, tone)
                    };
                    crate::widgets::compact_subcard(ui, None, true, |ui| {
                        let mut fill_clicked = false;
                        crate::widgets::responsive_compact_header(
                            ui,
                            360.0,
                            |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    if crate::widgets::clickable_mono_badge(
                                        ui,
                                        tag,
                                        if self.route_inbound.trim() == tag {
                                            crate::widgets::BadgeTone::Accent
                                        } else {
                                            crate::widgets::BadgeTone::Muted
                                        },
                                    )
                                    .on_hover_text("点击填入下方路由静态预测")
                                    .clicked()
                                    {
                                        fill_clicked = true;
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&detail).monospace().size(12.5),
                                        )
                                        .wrap(),
                                    );
                                });
                            },
                            |ui, _rtl| {
                                crate::widgets::badge(ui, status, tone);
                            },
                        );
                        if fill_clicked {
                            self.route_inbound = tag.to_owned();
                            if proto == "dokodemo-door" {
                                let net = if crate::port_forward::normalize_network(
                                    ib.pointer("/settings/network").and_then(|v| v.as_str()),
                                ) == "udp"
                                {
                                    "udp"
                                } else {
                                    "tcp"
                                };
                                let t_host = ib
                                    .pointer("/settings/address")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let t_port = ib
                                    .pointer("/settings/port")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                if !t_host.is_empty() {
                                    self.route_host = t_host.to_owned();
                                }
                                if t_port > 0 {
                                    self.ui.route_port = t_port.to_string();
                                }
                                self.route_network = net.to_owned();
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
            }
        });
        ui.add_space(16.0);
        card(ui, |ui| {
            ui.label(
                RichText::new("当前运行配置 · 路由静态预测")
                    .strong()
                    .size(18.0),
            );
            if let Some(id) = self.committed_id() {
                line(ui, "运行配置", self.ui.workspace.name(id));
            }
            ui.label(
                RichText::new(
                    "用途：核对某个目标会命中当前运行配置的哪条 routing 规则和出站；只做静态预测，不发起连接，也不代替实际访问。",
                )
                .small()
                .weak(),
            );
            let label_col = 116.0_f32.min(ui.available_width() * 0.30);
            let field_width = (ui.available_width() - label_col - 16.0).max(140.0);
            let (host_resp, port_resp) = egui::Grid::new("route-fields")
                .num_columns(2)
                .min_col_width(label_col)
                .min_row_height(CONTROL_HEIGHT)
                .spacing(egui::vec2(16.0, 10.0))
                .show(ui, |ui| {
                    ui.label("目标域名或 IP");
                    let host_resp = crate::widgets::singleline_select_all(
                        ui,
                        &mut self.route_host,
                        field_width,
                        "example.com",
                    );
                    ui.end_row();
                    ui.label("端口 / 网络");
                    let port_resp = ui
                        .horizontal(|ui| {
                            let port_resp = crate::widgets::singleline_select_all(
                                ui,
                                &mut self.ui.route_port,
                                100.0,
                                "",
                            );
                            egui::ComboBox::from_id_salt("route-network")
                                .width(100.0)
                                .selected_text(&self.route_network)
                                .show_ui(ui, |ui| {
                                    for network in ["tcp", "udp"] {
                                        ui.selectable_value(
                                            &mut self.route_network,
                                            network.into(),
                                            network,
                                        );
                                    }
                                });
                            port_resp
                        })
                        .inner;
                    ui.end_row();
                    (host_resp, port_resp)
                })
                .inner;
            struct RouteInboundShortcut {
                tag: String,
                forward_target: Option<(String, u16, String)>,
            }
            let apply_shortcut = |app: &mut App, shortcut: &RouteInboundShortcut| {
                app.route_inbound = shortcut.tag.clone();
                if let Some((host, port, net)) = &shortcut.forward_target {
                    app.route_host = host.clone();
                    if *port > 0 {
                        app.ui.route_port = port.to_string();
                    }
                    app.route_network = net.clone();
                }
            };
            let runtime_inbounds_route: Vec<RouteInboundShortcut> = self
                .ui
                .runtime
                .as_ref()
                .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.json).ok())
                .and_then(|v| {
                    v.get("inbounds")?.as_array().map(|arr| {
                        arr.iter()
                            .filter_map(|ib| {
                                let tag = ib.get("tag")?.as_str()?.trim();
                                if tag.is_empty() {
                                    return None;
                                }
                                let forward_target = if ib.get("protocol").and_then(|p| p.as_str())
                                    == Some("dokodemo-door")
                                {
                                    let host = ib
                                        .pointer("/settings/address")
                                        .and_then(|a| a.as_str())
                                        .unwrap_or("")
                                        .to_owned();
                                    let port = ib
                                        .pointer("/settings/port")
                                        .and_then(|p| p.as_u64())
                                        .and_then(|p| u16::try_from(p).ok())
                                        .unwrap_or(0);
                                    let net = if crate::port_forward::normalize_network(
                                        ib.pointer("/settings/network").and_then(|n| n.as_str()),
                                    ) == "udp"
                                    {
                                        "udp".to_owned()
                                    } else {
                                        "tcp".to_owned()
                                    };
                                    (!host.is_empty()).then_some((host, port, net))
                                } else {
                                    None
                                };
                                Some(RouteInboundShortcut {
                                    tag: tag.to_owned(),
                                    forward_target,
                                })
                            })
                            .collect()
                    })
                })
                .unwrap_or_default();
            if !runtime_inbounds_route.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("快捷入站：").small().weak());
                    if ui
                        .selectable_label(self.route_inbound.trim().is_empty(), "不指定")
                        .clicked()
                    {
                        self.route_inbound.clear();
                    }
                    for item in &runtime_inbounds_route {
                        let selected = self.route_inbound.trim() == item.tag;
                        if ui.selectable_label(selected, &item.tag).clicked() {
                            apply_shortcut(self, item);
                        }
                    }
                });
            }
            egui::CollapsingHeader::new("高级匹配条件（可选）").show(ui, |ui| {
                let adv_label_col = 116.0_f32.min(ui.available_width() * 0.30);
                let adv_field_width = (ui.available_width() - adv_label_col - 16.0).max(140.0);
                let gap_x = ui.spacing().item_spacing.x;
                egui::Grid::new("route-advanced-fields")
                    .num_columns(2)
                    .min_col_width(adv_label_col)
                    .min_row_height(CONTROL_HEIGHT)
                    .spacing(egui::vec2(16.0, 10.0))
                    .show(ui, |ui| {
                        ui.label("入站 Tag");
                        ui.vertical(|ui| {
                            crate::widgets::singleline_select_all(
                                ui,
                                &mut self.route_inbound,
                                adv_field_width,
                                "",
                            );
                            if !runtime_inbounds_route.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    for item in &runtime_inbounds_route {
                                        if ui.button(&item.tag).clicked() {
                                            apply_shortcut(self, item);
                                        }
                                    }
                                });
                            }
                        });
                        ui.end_row();
                        ui.label("已知目标 IP");
                        ui.add(singleline(&mut self.ui.route_ip, adv_field_width));
                        ui.end_row();
                        ui.label("来源 IP / 端口");
                        ui.horizontal(|ui| {
                            ui.add(singleline(
                                &mut self.ui.route_source,
                                (adv_field_width - 100.0 - gap_x).max(80.0),
                            ));
                            ui.add(singleline(&mut self.ui.route_source_port, 100.0));
                        });
                        ui.end_row();
                        ui.label("用户 / 协议");
                        let half_w = ((adv_field_width - gap_x) * 0.5).max(80.0);
                        ui.horizontal(|ui| {
                            ui.add(singleline(&mut self.ui.route_user, half_w));
                            ui.add(singleline(&mut self.ui.route_protocol, half_w));
                        });
                        ui.end_row();
                        ui.label("属性 key=value");
                        ui.add(singleline(&mut self.ui.route_attrs, adv_field_width));
                        ui.end_row();
                    });
                ui.label(
                    RichText::new(
                        "仅填写已知条件；留空表示该条件未知。DNS 解析、sniffing 和未支持的前序条件都可能使结果不确定。",
                    )
                    .small()
                    .weak(),
                );
            });
            let can_predict = self.ui.connected && self.is_running() && !self.busy;
            let enter = (host_resp.lost_focus() || port_resp.lost_focus())
                && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if primary(ui, "预测命中规则与出站", can_predict) || (enter && can_predict) {
                let port = if self.ui.route_port.trim().is_empty() {
                    Some(None)
                } else {
                    self.ui
                        .route_port
                        .trim()
                        .parse::<u16>()
                        .ok()
                        .filter(|p| *p > 0)
                        .map(Some)
                };
                let ip_valid = self.ui.route_ip.trim().is_empty()
                    || self.ui.route_ip.trim().parse::<std::net::IpAddr>().is_ok();
                if let Some(port) = port.filter(|_| ip_valid && !self.route_host.trim().is_empty())
                {
                    let optional = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
                    let source_port = optional(&self.ui.route_source_port)
                        .map(|v| v.parse::<u16>())
                        .transpose();
                    let attrs = optional(&self.ui.route_attrs)
                        .map(|text| {
                            text.lines()
                                .filter(|line| !line.trim().is_empty())
                                .map(|line| {
                                    line.split_once('=')
                                        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
                                        .ok_or(())
                                })
                                .collect::<std::result::Result<
                                    std::collections::BTreeMap<String, String>,
                                    (),
                                >>()
                        })
                        .transpose();
                    if source_port.is_err() || attrs.is_err() {
                        self.message = "来源端口或属性格式无效；属性请使用每行 key=value。".into();
                    } else {
                        let context = RouteContext {
                            host: self.route_host.trim().into(),
                            port,
                            inbound_tag: optional(&self.route_inbound),
                            network: optional(&self.route_network),
                            target_ip: optional(&self.ui.route_ip).or_else(|| {
                                self.route_host
                                    .trim()
                                    .parse::<std::net::IpAddr>()
                                    .ok()
                                    .map(|ip| ip.to_string())
                            }),
                            source_ip: optional(&self.ui.route_source),
                            source_port: source_port.unwrap_or_default(),
                            user: optional(&self.ui.route_user),
                            protocol: optional(&self.ui.route_protocol),
                            attrs: attrs.unwrap_or_default(),
                        };
                        if let Some(g) = self.status.as_ref().and_then(|s| s.generation.as_ref()) {
                            self.send(Command::TestRunningRoute {
                                generation_id: g.id.clone(),
                                context,
                            });
                        }
                    }
                } else {
                    self.message = "请输入目标、1–65535 的端口和有效的目标 IP（可留空）。".into();
                }
                crate::widgets::focus_and_select_all(ui.ctx(), &host_resp, &self.route_host);
            }
            ui.label(RichText::new("仅使用冻结的当前运行配置。域名解析、sniffing 或未支持的前序条件可能使结果未知。").small().weak());
            if let Some((_, context)) = &self.ui.route_tested {
                line(
                    ui,
                    "本次预测目标",
                    &format!(
                        "{}:{}",
                        context.host,
                        context
                            .port
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "未指定".into())
                    ),
                );
            }
            if let Ok(result) = serde_json::from_str::<RouteResult>(&self.route_result) {
                ui.separator();
                let outbound = result.expected_outbound.as_deref().unwrap_or("未知");
                let outbound_tone = match result.expected_outbound.as_deref() {
                    Some("direct") => crate::widgets::BadgeTone::Success,
                    Some("block") => crate::widgets::BadgeTone::Danger,
                    Some(_) => crate::widgets::BadgeTone::Accent,
                    None => crate::widgets::BadgeTone::Muted,
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("静态预测：").strong());
                    crate::widgets::badge(ui, outbound, outbound_tone);
                    let rule_text = result
                        .rule_index
                        .map(|i| format!("命中第 {} 条规则", i + 1))
                        .unwrap_or_else(|| "无确定命中".into());
                    crate::widgets::badge(ui, &rule_text, crate::widgets::BadgeTone::Muted);
                });
                line(ui, "静态预测", outbound);
                line(
                    ui,
                    "命中规则",
                    &result
                        .rule_index
                        .map(|i| (i + 1).to_string())
                        .unwrap_or_else(|| "无确定命中".into()),
                );
                egui::CollapsingHeader::new("预测依据与限制").show(ui, |ui| {
                    ui.label(result.reason);
                    ui.add(egui::Label::new(RichText::new(&self.route_result).monospace()).wrap());
                });
            }
        });
        ui.add_space(16.0);
        card(ui, |ui| {
            ui.label(RichText::new("出站 TCP 延迟探测").strong().size(18.0));
            ui.label(
                RichText::new(
                    "仅测量到目标主机和端口的 TCP 握手时间，不经过 Xray，也不代表远端服务可用。",
                )
                .small()
                .weak(),
            );
            ui.horizontal_wrapped(|ui| {
                ui.label("主机");
                let host_w = if ui.max_rect().width().is_finite() {
                    (ui.max_rect().width() * 0.42).clamp(180.0, 360.0)
                } else {
                    260.0
                };
                ui.add(singleline(&mut self.ui.latency_host, host_w));
                ui.label("端口");
                ui.add(singleline(&mut self.ui.latency_port, 80.0));
                if primary(
                    ui,
                    "开始探测",
                    self.ui.connected
                        && !self.busy
                        && !self.ui.latency_host.trim().is_empty()
                        && !self.ui.latency_port.trim().is_empty(),
                ) {
                    match self.ui.latency_port.trim().parse::<u16>() {
                        Ok(port) if port > 0 => {
                            self.send(Command::TestLatency {
                                host: self.ui.latency_host.trim().into(),
                                port,
                            });
                        }
                        _ => self.message = "端口必须是 1–65535 的数字。".into(),
                    }
                }
            });
            if let Ok(result) = serde_json::from_str::<serde_json::Value>(&self.ui.latency_result) {
                ui.separator();
                line(ui, "目标地址", result["address"].as_str().unwrap_or("未知"));
                line(
                    ui,
                    "TCP 握手",
                    &format!("{} ms", result["ms"].as_u64().unwrap_or_default()),
                );
                egui::CollapsingHeader::new("原始结果").show(ui, |ui| {
                    ui.monospace(&self.ui.latency_result);
                });
            } else if !self.ui.latency_result.is_empty() {
                ui.separator();
                ui.monospace(&self.ui.latency_result);
            }
        });
    }
    pub(in crate::app) fn log_rows(&self) -> Vec<(String, String)> {
        if self.ui.logs_paused {
            return self.ui.frozen_logs.clone();
        }
        let mut rows = vec![];
        if self.ui.log_source != 2 {
            for event in &self.ui.events {
                rows.push((
                    event.level.into(),
                    format!(
                        "[操作 / {}] [{}] {}{}",
                        event.level,
                        event.timestamp,
                        event.message,
                        if event.request.is_empty() {
                            String::new()
                        } else {
                            format!("\nrequest_id: {}", event.request)
                        }
                    ),
                ));
            }
        }
        if self.ui.log_source != 1 {
            for chunk in self.logs.iter().rev() {
                for line in chunk.lines() {
                    let lower = line.to_lowercase();
                    let level = if lower.contains("[error]") || lower.contains("[fatal]") {
                        "错误"
                    } else if lower.contains("[warning]") || lower.contains("[warn]") {
                        "警告"
                    } else if lower.contains("[info]") || lower.contains("[debug]") {
                        "信息"
                    } else {
                        "未标级别"
                    };
                    let formatted = proxy_runtime::localize_log_line(line);
                    rows.push((level.into(), format!("[Core] {formatted}")));
                }
            }
        }
        rows
    }

    pub(super) fn format_log_rows(rows: &[(String, String)]) -> String {
        rows.iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Reads persisted `logs/supervisor.log.1` (if present) and `logs/supervisor.log` in chronological order.
    pub(super) fn read_session_logs(&self) -> std::result::Result<Option<String>, String> {
        let logs_dir = self.root.join("logs");
        let mut combined = String::new();
        for name in ["supervisor.log.1", "supervisor.log"] {
            let path = logs_dir.join(name);
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let chunk = String::from_utf8_lossy(&bytes);
                    if !combined.is_empty() && !combined.ends_with('\n') && !chunk.is_empty() {
                        combined.push('\n');
                    }
                    for segment in chunk.split_inclusive('\n') {
                        let (line, ending) = match segment.strip_suffix('\n') {
                            Some(stripped) => (stripped, "\n"),
                            None => (segment, ""),
                        };
                        combined.push_str(&proxy_runtime::localize_log_line(line));
                        combined.push_str(ending);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("{}: {error}", display_path(&path))),
            }
        }
        if combined.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(combined))
        }
    }

    #[cfg(any(windows, test))]
    pub(super) fn write_log_export(
        path: &std::path::Path,
        content: &str,
    ) -> std::result::Result<(), String> {
        std::fs::write(path, content).map_err(|error| error.to_string())
    }

    pub(super) fn copy_session_logs(&mut self, ctx: &egui::Context) {
        match self.read_session_logs() {
            Ok(Some(text)) => {
                ctx.copy_text(text);
                self.message = "已复制持久化会话日志到剪贴板。".into();
            }
            Ok(None) => self.message = "暂无已落盘的会话日志。".into(),
            Err(error) => self.message = format!("读取会话日志失败：{error}"),
        }
    }

    #[cfg(windows)]
    pub(super) fn export_text_with_dialog(
        &mut self,
        default_name: &str,
        content: &str,
        ok_message: &str,
    ) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("日志文件", &["log", "txt"])
            .set_file_name(default_name)
            .save_file()
        {
            match Self::write_log_export(&path, content) {
                Ok(()) => self.message = ok_message.into(),
                Err(error) => self.message = format!("导出日志失败：{error}"),
            }
        }
    }

    #[cfg(windows)]
    pub(super) fn export_session_logs_with_dialog(&mut self) {
        match self.read_session_logs() {
            Ok(Some(text)) => {
                self.export_text_with_dialog(
                    "proxy-pilot-supervisor.log",
                    &text,
                    "已导出持久化会话日志。",
                );
            }
            Ok(None) => self.message = "暂无已落盘的会话日志。".into(),
            Err(error) => self.message = format!("读取会话日志失败：{error}"),
        }
    }

    pub(in crate::app) fn logs_page(&mut self, ui: &mut egui::Ui, viewport_height: f32) {
        let page_top = ui.cursor().top();
        heading(
            ui,
            "日志",
            "查看 Core 输出与界面操作记录，按消息或请求身份定位问题。",
        );
        if !self.profile_id.is_empty() {
            let can_edit = self.write_blocker().is_none()
                && !self.ui.draft_conflict
                && self.selected_profile == self.profile_id
                && self.ui.number_inputs.is_empty();
            ui.horizontal_wrapped(|ui| {
                ui.label("Core 日志输出级别");
                self.core_log_level_picker(ui, can_edit, "logs-core-loglevel");
                if let Some(running_level) = self.running_core_log_level()
                    && self.is_running()
                {
                    ui.label(
                        RichText::new(format!("运行中级别：{running_level}"))
                            .small()
                            .weak(),
                    );
                }
                let can_apply = can_edit && !self.core_path.is_empty();
                let needs_apply = self.running_core_log_level().as_deref()
                    != Some(self.core_log_level().as_str())
                    || !self.is_running();
                if needs_apply
                    && ui
                        .add_enabled(can_apply, egui::Button::new("应用到 Core"))
                        .on_hover_text("校验当前配置并将所选日志级别应用到运行中的 Core")
                        .clicked()
                {
                    self.validate_and_apply_draft();
                }
            });
            ui.add_space(6.0);
        }
        let search_width = (ui.available_width() - 312.0).max(120.0);
        ui.horizontal(|ui| {
            ui.label("搜索");
            ui.add(
                singleline(&mut self.ui.log_search, search_width).hint_text("搜索消息或请求 ID"),
            );
            egui::ComboBox::from_id_salt("log-level")
                .width(110.0)
                .selected_text(&self.ui.log_level)
                .show_ui(ui, |ui| {
                    for level in ["全部", "信息", "警告", "错误", "未标级别"] {
                        ui.selectable_value(&mut self.ui.log_level, level.into(), level);
                    }
                });
            ui.add_enabled_ui(!self.ui.logs_paused, |ui| {
                egui::ComboBox::from_id_salt("log-source")
                    .width(130.0)
                    .selected_text(["全部来源", "界面操作", "Core 输出"][self.ui.log_source])
                    .show_ui(ui, |ui| {
                        for (i, label) in ["全部来源", "界面操作", "Core 输出"].iter().enumerate()
                        {
                            ui.selectable_value(&mut self.ui.log_source, i, *label);
                        }
                    });
            });
        });
        let search = self.ui.log_search.to_lowercase();
        let rows: Vec<_> = self
            .log_rows()
            .into_iter()
            .filter(|(level, text)| {
                (self.ui.log_level == "全部" || *level == self.ui.log_level)
                    && text.to_lowercase().contains(&search)
            })
            .collect();
        let stats_text = format!(
            "{} 条结果{}",
            rows.len(),
            if self.ui.logs_paused {
                " · 显示已暂停"
            } else {
                ""
            }
        );
        enum LogBarAction {
            TogglePause,
            GetLogs,
            CopyRows,
            #[cfg(windows)]
            ExportLogs,
        }
        let mut log_action = None;
        let pause_label = if self.ui.logs_paused {
            "继续显示"
        } else {
            "暂停显示"
        };
        let has_rows = !rows.is_empty();
        crate::widgets::responsive_header(
            ui,
            560.0,
            |ui| {
                ui.label(RichText::new(&stats_text).small().weak());
            },
            |ui, rtl| {
                #[cfg(windows)]
                let indices: &[usize] = if rtl { &[3, 2, 1, 0] } else { &[0, 1, 2, 3] };
                #[cfg(not(windows))]
                let indices: &[usize] = if rtl { &[2, 1, 0] } else { &[0, 1, 2] };
                for &idx in indices {
                    match idx {
                        0 if ui.button(pause_label).clicked() => {
                            log_action = Some(LogBarAction::TogglePause);
                        }
                        1 if ui.button("读取 Core 日志").clicked() => {
                            log_action = Some(LogBarAction::GetLogs);
                        }
                        2 if ui
                            .add_enabled(has_rows, egui::Button::new("复制筛选结果"))
                            .clicked() =>
                        {
                            log_action = Some(LogBarAction::CopyRows);
                        }
                        #[cfg(windows)]
                        3 if ui
                            .add_enabled(has_rows, egui::Button::new("导出日志…"))
                            .clicked() =>
                        {
                            log_action = Some(LogBarAction::ExportLogs);
                        }
                        _ => {}
                    }
                }
            },
        );
        match log_action {
            Some(LogBarAction::TogglePause) => {
                if !self.ui.logs_paused {
                    self.ui.frozen_logs = self.log_rows();
                }
                self.ui.logs_paused = !self.ui.logs_paused;
            }
            Some(LogBarAction::GetLogs) => self.send(Command::GetLogs),
            Some(LogBarAction::CopyRows) => {
                ui.ctx().copy_text(Self::format_log_rows(&rows));
                self.message = format!("已复制 {} 条日志到剪贴板。", rows.len());
            }
            #[cfg(windows)]
            Some(LogBarAction::ExportLogs) => {
                let content = Self::format_log_rows(&rows);
                let count = rows.len();
                self.export_text_with_dialog(
                    "proxy-pilot-logs.txt",
                    &content,
                    &format!("已导出 {count} 条日志。"),
                );
            }
            None => {}
        }
        let log_height =
            (viewport_height - (ui.cursor().top() - page_top) - 110.0).clamp(180.0, 760.0);
        card(ui, |ui| {
            ui.set_min_height(log_height.min(230.0));
            if rows.is_empty() {
                ui.label(RichText::new("暂无匹配的日志。").weak());
            }
            egui::ScrollArea::vertical()
                .max_height(log_height)
                .id_salt("log-content")
                .show(ui, |ui| {
                    for (idx, (level, text)) in rows.iter().enumerate() {
                        let color = if level == "错误" {
                            ui.visuals().error_fg_color
                        } else if level == "警告" {
                            ui.visuals().warn_fg_color
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.push_id(idx, |ui| {
                            ui.horizontal(|ui| {
                                let text_width = (ui.available_width() - 68.0).max(120.0);
                                ui.allocate_ui(egui::vec2(text_width, 0.0), |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(text).monospace().color(color),
                                        )
                                        .wrap(),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::TOP),
                                    |ui| {
                                        if crate::widgets::compact_button(ui, "复制")
                                            .on_hover_text("复制此条日志")
                                            .clicked()
                                        {
                                            ui.ctx().copy_text(text.clone());
                                            self.message = "已复制该条日志到剪贴板。".into();
                                        }
                                    },
                                );
                            });
                        });
                        ui.separator();
                    }
                });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("持久化会话日志：").small().weak());
            if ui.button("复制会话日志").clicked() {
                self.copy_session_logs(ui.ctx());
            }
            #[cfg(windows)]
            if ui.button("导出会话日志…").clicked() {
                self.export_session_logs_with_dialog();
            }
        });
        ui.label(RichText::new("Core 输出与操作记录有容量上限，重启后不保留本次界面的操作列表。日志可能包含目标地址或认证信息。").small().weak());
    }
}
