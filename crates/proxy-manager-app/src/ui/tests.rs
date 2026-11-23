use super::*;

fn app() -> App {
    let (tx, _requests) = mpsc::sync_channel(MAX_PENDING);
    let (_responses, rx) = mpsc::channel();
    let mut app = App::with_channels("unused-ui-test".into(), tx, rx);
    app.ui.connected = true;
    app.ui.setup_initialized = true;
    app.ui.onboarding = false;
    app.ui.settings_loaded = true;
    app.ui.profiles_loaded = true;
    let json = default_config();
    let revision = hash(json.as_bytes());
    app.load(Profile {
        id: "work".into(),
        json: json.clone(),
        revision: revision.clone(),
    });
    app.profiles.push(ProfileSummary {
        id: "work".into(),
        revision: revision.clone(),
    });
    app.ui
        .workspace
        .names
        .insert("work".into(), "工作网络".into());
    app.core_path = "C:\\Apps\\Xray\\xray.exe".into();
    app.status = Some(Status {
        generation: Some(Generation {
            id: "generation-1".into(),
            transaction_id: "transaction".into(),
            request_id: "request".into(),
            candidate: Candidate {
                id: "candidate".into(),
                digest: "digest".into(),
                profile_id: "work".into(),
                base_revision: revision.clone(),
                revision,
                config_path: "config.json".into(),
                core: CoreIdentity {
                    path: "frozen/core".into(),
                    sha256: "sha".into(),
                    version: "Xray test".into(),
                },
                resources: Resources {
                    id: "resources".into(),
                    root: "frozen/resources".into(),
                    files: vec![],
                    complete: true,
                    notes: vec![],

                    dynamic: vec![],
                },
            },
            proxy_mode: ProxyMode::Direct,
            http_inbound_tag: "http-in".into(),
            running: true,

            pac: None,
        }),
        core_state: "Running".into(),
        proxy_state: "Unmanaged".into(),
        runtime_instance_id: Some("instance".into()),
        supervisor_epoch: "epoch".into(),
        last_error: None,
        recovery_required: false,
        winhttp: None,
        listener_state: "Ready".into(),
        pac_state: "Stopped".into(),
        auto_connect_notice: None,
        proxy_availability: None,
    });
    app.ui.runtime = Some(RuntimeConfig {
        generation_id: "generation-1".into(),
        json,
    });
    app
}
fn render(app: &mut App, ctx: &egui::Context, width: f32, height: f32) {
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        },
        |ctx| app.render(ctx),
    );
    assert!(!output.shapes.is_empty());
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    assert!(!primitives.is_empty());
}

#[test]
fn native_pages_and_dialogs_render_in_both_themes_at_supported_sizes() {
    let mut app = app();
    let ctx = egui::Context::default();
    configure(&ctx);
    for appearance in [Appearance::Light, Appearance::Dark] {
        appearance.apply(&ctx);
        for (width, height) in [(1100.0, 760.0), (760.0, 500.0)] {
            for page in [
                Page::Overview,
                Page::Profiles,
                Page::Pac,
                Page::Diagnostics,
                Page::Logs,
                Page::Settings,
            ] {
                app.ui.page = page;
                render(&mut app, &ctx, width, height);
            }
            for (tab, adv) in [
                (ProfileTab::Outbounds, AdvancedTab::Inbounds),
                (ProfileTab::Routing, AdvancedTab::Inbounds),
                (ProfileTab::Advanced, AdvancedTab::Inbounds),
                (ProfileTab::Advanced, AdvancedTab::Json),
                (ProfileTab::Advanced, AdvancedTab::History),
            ] {
                app.ui.page = Page::Profiles;
                app.ui.profile_tab = tab;
                app.ui.advanced_tab = adv;
                render(&mut app, &ctx, width, height);
            }
            for tab in [
                SettingsTab::Core,
                SettingsTab::Startup,
                SettingsTab::Probe,
                SettingsTab::Appearance,
                SettingsTab::Storage,
            ] {
                app.ui.page = Page::Settings;
                app.ui.settings_tab = tab;
                render(&mut app, &ctx, width, height);
            }
            for dialog in [
                Dialog::Confirm(Action::Stop),
                Dialog::Confirm(Action::Restart),
                Dialog::UnsavedExit(ExitKind::Shutdown),
                Dialog::Create,
            ] {
                app.ui.dialog = Some(dialog);
                render(&mut app, &ctx, width, height);
                app.ui.dialog = None;
            }
        }
    }
}

#[test]
fn onboarding_and_native_candidate_preview_render_without_mock_results() {
    let mut app = app();
    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.onboarding = true;
    app.ui.page = Page::Overview;
    for step in 0..3 {
        app.ui.setup_step = step;
        render(&mut app, &ctx, 760.0, 500.0);
    }
    let candidate = app
        .status
        .as_ref()
        .unwrap()
        .generation
        .as_ref()
        .unwrap()
        .candidate
        .clone();
    app.accept_candidate(candidate);
    render(&mut app, &ctx, 760.0, 500.0);
}

#[test]
fn runtime_proxy_actions_use_runtime_tags_instead_of_an_unapplied_draft() {
    let mut app = app();
    app.http_tag = "draft-only-tag".into();
    assert_eq!(app.runtime_http_tag(), "http-in");
    app.status.as_mut().unwrap().proxy_state = "Released".into();
    assert_eq!(app.proxy_label(), "未由本程序接管");
    app.ui.connected = false;
    assert_eq!(app.proxy_label(), "状态待确认");
}

#[test]
fn log_pause_retains_a_snapshot_and_level_detection_does_not_invent_core_levels() {
    let mut app = app();
    app.logs = vec!["unmarked output\n[Warning] retry".into()];
    let rows = app.log_rows();
    assert!(rows.iter().any(|(level, _)| level == "未标级别"));
    app.ui.frozen_logs = rows.clone();
    app.ui.logs_paused = true;
    app.logs.clear();
    app.add_event("信息", "later", String::new());
    assert_eq!(app.log_rows(), rows);
    app.ui.logs_paused = false;
    assert_ne!(app.log_rows(), rows);
}

#[test]
fn pointer_navigation_and_preview_only_apply_after_confirmation() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 760.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    fn text_position(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| text_position(s, label)),
            _ => None,
        }
    }
    let click = |app: &mut App, label: &str| {
        // Settle layout before targeting the rendered label, then send real egui input.
        frame(app, vec![]);
        let output = frame(app, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| text_position(&s.shape, label))
            .unwrap_or_else(|| panic!("Missing button: {label}"));
        frame(
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            app,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    };
    click(&mut app, "配置");
    assert!(app.ui.page == Page::Profiles);
    assert!(requests.try_iter().all(|r| !r.command.is_write()));
    let candidate = app
        .status
        .as_ref()
        .unwrap()
        .generation
        .as_ref()
        .unwrap()
        .candidate
        .clone();
    app.accept_candidate(candidate.clone());
    click(&mut app, "返回编辑");
    assert!(!app.ui.preview_open);
    assert!(requests.try_iter().all(|r| !r.command.is_write()));
    app.accept_candidate(candidate.clone());
    click(&mut app, "确认应用");
    assert!(requests.try_iter().any(|r| matches!(
        r.command,
        Command::ApplyProfile { candidate_id, .. } if candidate_id == candidate.id
    )));
    assert!(app.busy);
}

#[cfg(windows)]
#[test]
fn windows_path_fields_and_clipboard_hide_prefixes_without_rewriting_paths() {
    let mut app = app();
    app.root = PathBuf::from(r"\\?\D:\Proxy Pilot 数据\data");
    app.core_path = r"\\?\C:\Xray\folder.\xray.exe".into();
    app.asset_root = r"\\?\UNC\server\share\资源".into();
    let core = app.core_path.clone();
    let assets = app.asset_root.clone();
    app.ui.saved_settings = Some(Settings {
        core_path: core.clone().into(),
        ..Default::default()
    });
    app.ui.page = Page::Settings;
    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 760.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    fn label_position(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
        output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
    }
    frame(&mut app, vec![]);
    let output = frame(&mut app, vec![]);
    assert!(label_position(&output, r"C:\Xray\folder.\xray.exe").is_some());
    assert!(label_position(&output, r"\\server\share\资源").is_some());
    assert!(label_position(&output, &core).is_none());
    assert_eq!(app.core_path, core);
    assert_eq!(app.asset_root, assets);
    assert_eq!(
        app.ui.saved_settings.as_ref().unwrap().core_path,
        PathBuf::from(&core)
    );
    app.ui.settings_tab = SettingsTab::Storage;
    frame(&mut app, vec![]);
    let output = frame(&mut app, vec![]);
    assert!(label_position(&output, r"D:\Proxy Pilot 数据\data").is_some());
    let pos = label_position(&output, "复制目录路径").unwrap();
    frame(
        &mut app,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    let output = frame(
        &mut app,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(
        output
            .platform_output
            .commands
            .iter()
            .any(|command| matches!(
                command, egui::OutputCommand::CopyText(text) if text == r"D:\Proxy Pilot 数据\data"
            ))
    );
    assert_eq!(app.root, PathBuf::from(r"\\?\D:\Proxy Pilot 数据\data"));
}

fn text_bounds(output: &egui::FullOutput, label: &str) -> egui::Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing rendered label: {label}"))
}

fn enclosing_rect(output: &egui::FullOutput, label: &str, min_height: f32) -> egui::Rect {
    let text = text_bounds(output, label);
    let mut rects = vec![];
    for shape in &output.shapes {
        if matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text() == label) {
            break;
        }
        painted_rects(&shape.shape, &mut rects);
    }
    rects
        .into_iter()
        .rev()
        .find(|rect| rect.contains(text.center()) && rect.height() >= min_height - 0.1)
        .unwrap()
}

fn painted_rects(shape: &egui::Shape, rects: &mut Vec<egui::Rect>) {
    match shape {
        egui::Shape::Rect(rect) => rects.push(rect.rect),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                painted_rects(shape, rects);
            }
        }
        _ => {}
    }
}

fn capture_layout(
    ctx: &egui::Context,
    output: egui::FullOutput,
    name: &str,
    width: f32,
    height: f32,
) {
    let Some(root) = std::env::var_os("XRAY_UI_CAPTURE_DIR") else {
        return;
    };
    let root = PathBuf::from(root);
    std::fs::create_dir_all(&root).unwrap();
    let atlas = ctx.fonts(|fonts| fonts.image());
    std::fs::write(
        root.join(format!("{name}.rgba")),
        atlas
            .pixels
            .iter()
            .flat_map(|p| p.to_array())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let meshes: Vec<_> = ctx.tessellate(output.shapes, output.pixels_per_point).into_iter()
        .map(|primitive| {
            let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else { panic!("Unexpected callback"); };
            assert_eq!(mesh.texture_id, egui::TextureId::Managed(0));
            serde_json::json!({
                "clip": [primitive.clip_rect.min.x, primitive.clip_rect.min.y, primitive.clip_rect.max.x, primitive.clip_rect.max.y],
                "indices": mesh.indices,
                "vertices": mesh.vertices.iter().map(|v| [v.pos.x,v.pos.y,v.uv.x,v.uv.y,
                    v.color.r() as f32/255.0,v.color.g() as f32/255.0,v.color.b() as f32/255.0,v.color.a() as f32/255.0]).collect::<Vec<_>>()
            })
        }).collect();
    std::fs::write(
        root.join(format!("{name}.json")),
        serde_json::to_vec(&serde_json::json!({
            "width":width, "height":height, "atlas_size":atlas.size, "meshes":meshes
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn toolbars_and_profile_cards_align_at_all_supported_widths() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for width in [760.0, 980.0, 1100.0] {
            let mut app = app();
            let ctx = egui::Context::default();
            configure(&ctx);
            appearance.apply(&ctx);
            let frame = |app: &mut App| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 760.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| app.render(ctx),
                )
            };
            let settle = |app: &mut App| {
                frame(app);
                frame(app);
                frame(app)
            };
            app.ui.page = Page::Logs;
            let output = settle(&mut app);
            let filter = ["搜索消息或请求 ID", "全部", "全部来源"]
                .map(|s| enclosing_rect(&output, s, CONTROL_HEIGHT));
            for rect in filter {
                assert!(
                    (rect.height() - CONTROL_HEIGHT).abs() < 1.0,
                    "filter height: {rect:?}"
                );
                assert!(
                    (rect.center().y - filter[0].center().y).abs() < 1.0,
                    "filter alignment: {filter:?}"
                );
                assert!(rect.right() <= width - 20.0, "filter overflow: {rect:?}");
            }
            for pair in filter.windows(2) {
                assert!(pair[0].right() < pair[1].left());
            }
            let actions = ["暂停显示", "读取 Core 日志", "复制筛选结果"]
                .map(|s| enclosing_rect(&output, s, CONTROL_HEIGHT));
            for rect in actions {
                assert!((rect.height() - CONTROL_HEIGHT).abs() < 1.0);
                assert!((rect.center().y - actions[0].center().y).abs() < 1.0);
            }
            assert!(
                (text_bounds(&output, "0 条结果").center().y - actions[0].center().y).abs() < 1.0
            );
            for pair in actions.windows(2) {
                assert!(pair[0].right() < pair[1].left());
            }
            capture_layout(
                &ctx,
                output,
                &format!("{appearance:?}-{width}-logs"),
                width,
                760.0,
            );

            app.ui.page = Page::Profiles;
            app.profile_id.clear();
            app.selected_profile.clear();
            app.profiles.clear();
            let output = settle(&mut app);
            let actions =
                ["新建", "导入", "刷新"].map(|s| enclosing_rect(&output, s, CONTROL_HEIGHT));
            if width >= 980.0 {
                for rect in actions {
                    assert!((rect.left() - actions[0].left()).abs() < 1.0);
                    assert!((rect.width() - actions[0].width()).abs() < 1.0);
                }
                for pair in actions.windows(2) {
                    assert!(pair[0].bottom() < pair[1].top());
                }
                let picker = enclosing_rect(&output, "我的配置 · 0", CONTROL_HEIGHT);
                let editor = enclosing_rect(&output, "添加第一份配置", CONTROL_HEIGHT);
                assert!((picker.top() - editor.top()).abs() < 1.0);
                assert!(
                    (picker.bottom() - editor.bottom()).abs() < 1.0,
                    "empty cards: {picker:?}, {editor:?}"
                );
                assert!(picker.right() < editor.left());
            } else {
                for rect in actions {
                    assert!((rect.top() - actions[0].top()).abs() < 1.0);
                }
                for pair in actions.windows(2) {
                    assert!(pair[0].right() < pair[1].left());
                }
            }
            capture_layout(
                &ctx,
                output,
                &format!("{appearance:?}-{width}-empty-profiles"),
                width,
                760.0,
            );
            // Also capture the remaining pages using the same real egui fonts and meshes.
            for (page, name) in [
                (Page::Overview, "overview"),
                (Page::Diagnostics, "diagnostics"),
                (Page::Settings, "settings"),
            ] {
                app.ui.page = page;
                let output = settle(&mut app);
                if width >= 980.0 && page == Page::Overview {
                    let nav = ["总览", "配置", "PAC", "日志", "诊断", "设置"]
                        .map(|s| text_bounds(&output, s));
                    for pair in nav.windows(2) {
                        assert!(
                            pair[0].right() < pair[1].left(),
                            "nav order mismatch: {nav:?}"
                        );
                    }
                }
                capture_layout(
                    &ctx,
                    output,
                    &format!("{appearance:?}-{width}-{name}"),
                    width,
                    760.0,
                );
            }
        }
    }
}

#[test]
fn dialogs_stay_inside_the_smallest_window_in_both_themes() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for (dialog, title, action, name) in [
            (Dialog::Create, "添加 / 导入配置", "添加配置", "import"),
            (
                Dialog::Confirm(Action::Restart),
                "启动 / 重启上次应用版本？",
                "启动 / 重启",
                "restart",
            ),
        ] {
            let mut app = app();
            app.ui.import_json = true;
            app.ui.new_name = "中文配置".into();
            app.ui.dialog = Some(dialog);
            let ctx = egui::Context::default();
            configure(&ctx);
            appearance.apply(&ctx);
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(760.0, 500.0));
            let mut output = None;
            for frame in 0..12 {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(viewport),
                        time: Some(frame as f64 * 0.05),
                        ..Default::default()
                    },
                    |ctx| app.render(ctx),
                ));
            }
            let output = output.unwrap();
            // Modal shapes follow the underlying page; the page itself may also
            // contain the title's coordinates, so inspect paint order here.
            let title_bounds = text_bounds(&output, title);
            let mut rects = vec![];
            for shape in &output.shapes {
                painted_rects(&shape.shape, &mut rects);
            }
            let modal = rects
                .into_iter()
                .rev()
                .find(|rect| {
                    rect.contains(title_bounds.center()) && rect.height() >= CONTROL_HEIGHT * 2.0
                })
                .unwrap();
            assert!(modal.width() < viewport.width() - 20.0);
            assert!(viewport.contains_rect(modal), "dialog overflow: {modal:?}");
            // The footer must be visible and use the same height as other controls.
            let cancel = enclosing_rect(&output, "取消", CONTROL_HEIGHT);
            let confirm = enclosing_rect(&output, action, CONTROL_HEIGHT);
            assert!(
                modal.contains_rect(cancel),
                "{name}: modal {modal:?}, cancel {cancel:?}"
            );
            assert!(
                modal.contains_rect(confirm),
                "{name}: modal {modal:?}, confirm {confirm:?}"
            );
            assert!((confirm.height() - CONTROL_HEIGHT).abs() < 1.0);
            assert!(
                (cancel.center().y - confirm.center().y).abs() < 1.0,
                "{name}: cancel {cancel:?}, confirm {confirm:?}"
            );
            assert!(cancel.right() < confirm.left());
            capture_layout(
                &ctx,
                output,
                &format!("{appearance:?}-760-{name}"),
                760.0,
                500.0,
            );
        }
    }
}

#[test]
fn long_incomplete_import_keeps_title_and_footer_visible_and_saves_original_only() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for (width, height) in [(760.0, 500.0), (1100.0, 760.0)] {
            let mut app = app();
            let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
            app.tx = tx;
            app.pending.clear();
            app.begin_create(true, false);
            app.ui.new_name = "待补充配置".into();
            app.ui.import_source = Some(std::env::temp_dir().join("config.json"));
            let assets = app.asset_root.clone();
            app.ui.new_json = serde_json::to_string_pretty(&serde_json::json!({
                "metadata":{"long_line":"keep-native-value-".repeat(100)},
                "outbounds":(0..50).map(|i| serde_json::json!({"tag":format!("node-{i}")})).collect::<Vec<_>>(),
                "routing":{"rules":[{"outboundTag":"api"}]}
            })).unwrap();
            let original = app.ui.new_json.clone();
            app.ui.failure = Some(Failure {
                error: Error::new("STATIC_VALIDATION_FAILED", "previous import failed"),
                request: String::new(),
            });
            let ctx = egui::Context::default();
            configure(&ctx);
            appearance.apply(&ctx);
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
            let frame = |app: &mut App, events, time| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(viewport),
                        events,
                        time: Some(time),
                        ..Default::default()
                    },
                    |ctx| app.render(ctx),
                )
            };
            let mut output = frame(&mut app, vec![], 0.0);
            for n in 1..12 {
                output = frame(&mut app, vec![], n as f64 * 0.05);
            }
            for label in ["添加 / 导入配置", "取消", "保存待补充配置"] {
                let bounds = text_bounds(&output, label);
                let clip = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(t) if t.galley.text() == label => Some(shape.clip_rect),
                        _ => None,
                    })
                    .unwrap();
                assert!(viewport.contains_rect(bounds), "viewport clips {label}");
                assert!(clip.contains_rect(bounds), "scroll area clips {label}");
                if label != "添加 / 导入配置" {
                    assert!(clip.contains_rect(enclosing_rect(&output, label, CONTROL_HEIGHT)));
                }
            }
            let button = text_bounds(&output, "保存待补充配置").center();
            capture_layout(
                &ctx,
                output,
                &format!("{appearance:?}-{width}-incomplete-import"),
                width,
                height,
            );
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(button),
                    egui::Event::PointerButton {
                        pos: button,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                0.7,
            );
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: button,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                0.8,
            );
            let sent: Vec<_> = requests.try_iter().collect();
            assert_eq!(app.asset_root, assets);
            assert!(sent.iter().any(
                |r| matches!(&r.command, Command::CreateProfile { json, .. } if json == &original)
            ));
            assert!(!sent.iter().any(|r| matches!(
                r.command,
                Command::ApplyProfile { .. } | Command::RestartCore
            )));
        }
    }
}

fn json_viewport_rects(output: &egui::FullOutput) -> Vec<egui::Rect> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if [
                    Color32::from_rgb(13, 18, 25),
                    Color32::from_rgb(245, 247, 250),
                ]
                .contains(&rect.fill) =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn profile_editors_follow_parent_size_for_short_and_long_documents_and_resizing() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for long in [false, true] {
            let mut app = app();
            app.ui.page = Page::Profiles;
            app.ui.profile_tab = ProfileTab::Advanced;
            app.ui.advanced_tab = AdvancedTab::Json;
            app.editor = if long {
                serde_json::to_string_pretty(&serde_json::json!({"metadata": {
                    "long_line": "native-value-".repeat(200),
                    "rows": (0..200).collect::<Vec<_>>()
                }}))
                .unwrap()
            } else {
                "{}".into()
            };
            let original = app.editor.clone();
            let ctx = egui::Context::default();
            configure(&ctx);
            appearance.apply(&ctx);
            let mut large = None;
            for (width, height) in [
                (1100.0, 760.0),
                (1920.0, 1040.0),
                (1440.0, 900.0),
                (760.0, 500.0),
                (1920.0, 1040.0),
            ] {
                let mut output = None;
                for _ in 0..12 {
                    output = Some(ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, height),
                            )),
                            events: if height == 500.0 {
                                vec![
                                    egui::Event::PointerMoved(egui::pos2(
                                        width - 30.0,
                                        height - 100.0,
                                    )),
                                    egui::Event::MouseWheel {
                                        unit: egui::MouseWheelUnit::Point,
                                        delta: egui::vec2(0.0, -100.0),
                                        modifiers: egui::Modifiers::NONE,
                                    },
                                ]
                            } else {
                                vec![]
                            },
                            ..Default::default()
                        },
                        |ctx| app.render(ctx),
                    ));
                }
                let output = output.unwrap();
                let editors = json_viewport_rects(&output);
                assert_eq!(editors.len(), 1);
                let editor = editors[0];
                let footer = enclosing_rect(&output, "检查并应用", CONTROL_HEIGHT);
                assert!(editor.height() >= 79.0, "collapsed editor: {editor:?}");
                assert!(
                    editor.width() > 250.0 && editor.right() <= width - 24.0,
                    "editor width: {editor:?}"
                );
                assert!(
                    editor.bottom() < footer.top(),
                    "editor overlaps actions: {editor:?}, {footer:?}"
                );
                if width == 1920.0 {
                    assert!(
                        editor.height() > 350.0,
                        "unused parent height: {editor:?}, footer: {footer:?}"
                    );
                    assert!(editor.width() > 1200.0, "unused parent width: {editor:?}");
                    assert!(footer.bottom() < height - 45.0);
                    if let Some(previous) = large {
                        assert_eq!(
                            editor.size(),
                            previous,
                            "resize did not recover original viewport size"
                        );
                    }
                    large = Some(editor.size());
                }
                assert_eq!(app.editor, original);
                capture_layout(
                    &ctx,
                    output,
                    &format!(
                        "{appearance:?}-{width}-adaptive-profile-{}",
                        if long { "long" } else { "short" }
                    ),
                    width,
                    height,
                );
            }
        }
    }
}

#[test]
fn json_editor_fills_nested_parent_and_blank_area_accepts_input() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let ctx = egui::Context::default();
        configure(&ctx);
        appearance.apply(&ctx);
        let mut json = "{}".to_string();
        let mut time = 0.0;
        for size in [
            egui::vec2(320.0, 120.0),
            egui::vec2(850.0, 480.0),
            egui::vec2(400.0, 240.0),
        ] {
            let parent = egui::Rect::from_min_size(egui::pos2(50.0, 50.0), size);
            let mut frame = |events| {
                time += 0.05;
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 760.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.scope_builder(egui::UiBuilder::new().max_rect(parent), |ui| {
                                json_editor(ui, &mut json, "nested-json", ui.available_height());
                            });
                        });
                    },
                )
            };
            let mut output = frame(vec![]);
            for _ in 0..4 {
                output = frame(vec![]);
            }
            let rects = json_viewport_rects(&output);
            assert_eq!(rects.len(), 1);
            assert!((rects[0].width() - size.x).abs() < 1.0);
            assert!((rects[0].height() - size.y).abs() < 1.0);
            let pos = parent.center();
            frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            frame(vec![egui::Event::Text(" ".into())]);
        }
        assert_eq!(
            json, "{}   ",
            "blank editor space must be an editable surface"
        );
    }
}

#[test]
fn import_and_preview_json_viewports_grow_with_their_dialogs() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let mut previous_import = 0.0;
        let mut previous_preview = 0.0;
        for (width, height) in [(760.0, 500.0), (1100.0, 760.0), (1440.0, 1000.0)] {
            let ctx = egui::Context::default();
            configure(&ctx);
            appearance.apply(&ctx);
            let mut app = app();
            app.begin_create(true, false);
            app.ui.new_json = "{}".into();
            let frame = |app: &mut App| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, height),
                        )),
                        ..Default::default()
                    },
                    |ctx| app.render(ctx),
                )
            };
            let mut output = frame(&mut app);
            for _ in 0..12 {
                output = frame(&mut app);
            }
            let editors = json_viewport_rects(&output);
            assert_eq!(editors.len(), 1);
            assert!(editors[0].height() > previous_import, "import did not grow");
            previous_import = editors[0].height();
            assert!(text_bounds(&output, "取消").bottom() < height);
            capture_layout(
                &ctx,
                output,
                &format!("{appearance:?}-{width}-adaptive-import"),
                width,
                height,
            );
            app.ui.dialog = None;
            let candidate = app
                .status
                .as_ref()
                .unwrap()
                .generation
                .as_ref()
                .unwrap()
                .candidate
                .clone();
            app.accept_candidate(candidate);
            let mut output = frame(&mut app);
            for _ in 0..12 {
                output = frame(&mut app);
            }
            let editors = json_viewport_rects(&output);
            assert_eq!(editors.len(), 2);
            assert!(editors[0].height() >= previous_preview, "preview shrank");
            if height == 1000.0 {
                assert!(editors[0].height() > previous_preview);
            }
            previous_preview = editors[0].height();
            assert!((editors[0].height() - editors[1].height()).abs() < 1.0);
            assert!(editors[0].right() < editors[1].left());
            assert!(text_bounds(&output, "确认应用").bottom() < height);
            capture_layout(
                &ctx,
                output,
                &format!("{appearance:?}-{width}-adaptive-preview"),
                width,
                height,
            );
        }
    }
}

fn preview_warning_texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text().starts_with("尚未覆盖字段：") => {
                Some((
                    text.galley.text().to_string(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                ))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn long_preview_diagnostics_scroll_without_overlap_and_new_candidate_starts_at_top() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for (width, height) in [(760.0, 500.0), (1100.0, 760.0), (1920.0, 1040.0)] {
            let ctx = egui::Context::default();
            configure(&ctx);
            appearance.apply(&ctx);
            let mut app = app();
            app.editor = serde_json::to_string_pretty(&serde_json::json!({
                "metadata":{"long_line":"fixture-".repeat(200),"rows":(0..180).collect::<Vec<_>>()}
            }))
            .unwrap();
            let mut candidate = app
                .status
                .as_ref()
                .unwrap()
                .generation
                .as_ref()
                .unwrap()
                .candidate
                .clone();
            candidate.revision = hash(app.editor.as_bytes());
            candidate.resources.complete = false;
            candidate.resources.notes = (0..40).map(|n| format!("Uncovered dependency capability /vendor/field-{n}/{}; automatic recovery disabled", "nested/".repeat(12))).collect();
            app.accept_candidate(candidate.clone());
            let mut time = 0.0;
            let mut frame = |app: &mut App, events| {
                time += 0.05;
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, height),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| app.render(ctx),
                )
            };
            let settle = |app: &mut App, frame: &mut dyn FnMut(&mut App, Vec<egui::Event>) -> egui::FullOutput| {
                let mut output = frame(app, vec![]);
                for _ in 0..12 { output = frame(app, vec![]); }
                output
            };
            let mut output = settle(&mut app, &mut frame);
            let modal = enclosing_rect(&output, "预览本次应用", 36.0);
            let point = modal.center();
            let wheel = |point, dy| {
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, dy),
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            for _ in 0..8 {
                output = frame(&mut app, wheel(point, -400.0));
            }
            let title = "查看 40 项资源与自动恢复限制";
            let toggle = text_bounds(&output, title).center();
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(toggle),
                    egui::Event::PointerButton {
                        pos: toggle,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: toggle,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            output = settle(&mut app, &mut frame);
            let mut saw_last = false;
            for _ in 0..50 {
                let notes = preview_warning_texts(&output);
                assert!(!notes.is_empty(), "expanded warning details missing");
                for pair in notes.windows(2) {
                    assert!(
                        pair[0].1.bottom() < pair[1].1.top(),
                        "overlapping notes: {pair:?}"
                    );
                }
                saw_last |= notes.iter().any(|(t, _)| t.contains("/field-39/"));
                let pos = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text)
                            if text.galley.text().starts_with("尚未覆盖字段：") =>
                        {
                            let bounds = egui::Rect::from_min_size(text.pos, text.galley.size())
                                .intersect(shape.clip_rect);
                            (bounds.height() > 2.0).then_some(bounds)
                        }
                        _ => None,
                    })
                    .max_by(|a, b| a.height().total_cmp(&b.height()))
                    .unwrap()
                    .center();
                output = frame(&mut app, wheel(pos, -160.0));
            }
            assert!(saw_last, "last warning must be reachable by scrolling");
            for label in ["预览本次应用", "返回编辑", "确认应用"] {
                let bounds = text_bounds(&output, label);
                assert!(bounds.top() > 0.0 && bounds.bottom() < height);
            }
            capture_layout(
                &ctx,
                output,
                &format!("{appearance:?}-{width}-preview-many-notes"),
                width,
                height,
            );
            frame(&mut app, vec![egui::Event::PointerMoved(egui::Pos2::ZERO)]);
            settle(&mut app, &mut frame);
            app.ui.preview_open = false;
            candidate.id = "fresh-candidate".into();
            app.accept_candidate(candidate);
            let output = settle(&mut app, &mut frame);
            let target = text_bounds(&output, "目标配置");
            assert!(target.top() > text_bounds(&output, "预览本次应用").bottom());
            assert!(target.bottom() < json_viewport_rects(&output)[0].top());
            assert!(
                preview_warning_texts(&output).is_empty(),
                "fresh details should be collapsed"
            );
            let summary = text_bounds(
                &output,
                "配置已通过 Core 校验。未分类字段无法冻结；启动和恢复时仍需 Core 接受，失败会停止并报告。",
            );
            assert!(summary.top() > target.bottom());
            assert!(summary.bottom() < json_viewport_rects(&output)[0].top());
            capture_layout(
                &ctx,
                output,
                &format!("{appearance:?}-{width}-preview-limit-summary"),
                width,
                height,
            );
        }
    }
}

fn overlapping_visible_text(output: &egui::FullOutput) -> Vec<(String, String)> {
    let mut labels = vec![];
    for shape in &output.shapes {
        if let egui::Shape::Text(text) = &shape.shape {
            if text.galley.text().trim().is_empty() {
                continue;
            }
            // Respect right alignment and wrapped-row indentation. A whole
            // galley rectangle would report intentional empty space as overlap.
            for row in &text.galley.rows {
                let rect = row
                    .visuals
                    .mesh_bounds
                    .translate(text.pos.to_vec2() + row.pos.to_vec2())
                    .intersect(shape.clip_rect);
                if rect.width() > 2.0 && rect.height() > 2.0 {
                    labels.push((text.galley.text(), rect));
                }
            }
        }
    }
    let mut overlaps = vec![];
    for (index, (a, ar)) in labels.iter().enumerate() {
        for (b, br) in labels.iter().skip(index + 1) {
            let intersection = ar.intersect(*br);
            if intersection.width() > 2.0 && intersection.height() > 2.0 {
                overlaps.push((a.chars().take(60).collect(), b.chars().take(60).collect()));
            }
        }
    }
    overlaps
}

#[test]
fn long_content_on_all_pages_keeps_visible_text_separate() {
    let mut failures = vec![];
    for appearance in [Appearance::Light, Appearance::Dark] {
        for (width, height) in [(760.0, 500.0), (1100.0, 760.0), (1920.0, 1040.0)] {
            let mut cases = vec![
                (
                    Page::Overview,
                    ProfileTab::Outbounds,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
                (
                    Page::Diagnostics,
                    ProfileTab::Outbounds,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
                (
                    Page::Logs,
                    ProfileTab::Outbounds,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
            ];
            cases.extend(
                [
                    (ProfileTab::Outbounds, AdvancedTab::Inbounds),
                    (ProfileTab::Routing, AdvancedTab::Inbounds),
                    (ProfileTab::Advanced, AdvancedTab::Inbounds),
                    (ProfileTab::Advanced, AdvancedTab::Json),
                    (ProfileTab::Advanced, AdvancedTab::History),
                ]
                .map(|(tab, adv)| (Page::Profiles, tab, adv, SettingsTab::Core)),
            );
            cases.extend(
                [
                    SettingsTab::Core,
                    SettingsTab::Startup,
                    SettingsTab::Appearance,
                    SettingsTab::Storage,
                ]
                .map(|tab| {
                    (
                        Page::Settings,
                        ProfileTab::Outbounds,
                        AdvancedTab::Inbounds,
                        tab,
                    )
                }),
            );
            for (page, profile_tab, advanced_tab, settings_tab) in cases {
                let ctx = egui::Context::default();
                configure(&ctx);
                appearance.apply(&ctx);
                let mut app = app();
                app.ui
                    .workspace
                    .names
                    .insert("work".into(), "长配置显示名称".repeat(10));
                app.root = PathBuf::from(format!(
                    "C:\\Long data path\\{}",
                    "fixture-directory\\".repeat(20)
                ));
                app.core_path = format!(
                    "C:\\Long core path\\{}xray.exe",
                    "fixture-directory\\".repeat(20)
                );
                app.asset_root =
                    format!("C:\\Long asset path\\{}", "fixture-directory\\".repeat(20));
                app.ui.connected = false;
                app.status.as_mut().unwrap().recovery_required = true;
                app.ui.failure = Some(Failure {
                    error: Error::new("TEST_ERROR", "long error evidence\n".repeat(40)),
                    request: "request-test".into(),
                });
                app.ui.page = page;
                app.ui.profile_tab = profile_tab;
                app.ui.advanced_tab = advanced_tab;
                app.ui.settings_tab = settings_tab;
                app.status.as_mut().unwrap().last_error = Some(Error::new(
                    "RECOVERY_REQUIRED",
                    "recovery evidence\n".repeat(30),
                ));
                app.ui.operation_details = "fixture operation evidence\n".repeat(40);
                app.route_result = serde_json::to_string(&RouteResult {
                    kind: "StaticPrediction".into(),
                    match_state: "Unknown".into(),
                    rule_index: None,
                    expected_outbound: None,
                    actual_outbound: "Unverified".into(),
                    reason: "预测依据需要核对。\n".repeat(30),
                })
                .unwrap();
                let entries: Vec<_> = (0..20)
                    .map(|n| {
                        serde_json::json!({
                            "tag":format!("entry-{n}-{}", "very-long-native-tag-".repeat(4)),
                            "protocol":"socks",
                            "native_field_name_requiring_wrap_".repeat(8):"fixture-value",
                        })
                    })
                    .collect();
                app.editor = serde_json::to_string_pretty(&serde_json::json!({
                    "inbounds":entries,"outbounds":entries,"routing":{"rules":entries}
                }))
                .unwrap();
                app.history = (0..20)
                    .map(|n| {
                        let mut g = app
                            .status
                            .as_ref()
                            .unwrap()
                            .generation
                            .as_ref()
                            .unwrap()
                            .clone();
                        g.id = format!("history-{n}");
                        g.candidate.core.version =
                            "Xray fixture long version description ".repeat(8);
                        g
                    })
                    .collect();
                app.dependency_report.installations = (0..12)
                    .map(|n| crate::dependencies::Installation {
                        version: format!("v26.11.{n}"),
                        install_id: format!("fixture-{n}"),
                        valid: false,
                        error: Some("fixture dependency error\n".repeat(30)),
                    })
                    .collect();
                app.dependency_message = "依赖检查结果需要继续核对。\n".repeat(40);
                app.logs = vec!["[warning] fixture long log line ".repeat(20); 60];
                let frame = |app: &mut App, events| {
                    ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, height),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ctx| app.render(ctx),
                    )
                };
                let mut output = frame(&mut app, vec![]);
                for _ in 0..12 {
                    output = frame(&mut app, vec![]);
                }
                let mut expanded = std::collections::HashSet::new();
                for step in 0..24 {
                    let overlaps = overlapping_visible_text(&output);
                    if !overlaps.is_empty() {
                        failures.push(format!(
                            "{appearance:?} {width} {page:?} / {profile_tab:?} / {settings_tab:?} step {step}: {:?}",
                            overlaps.into_iter().take(6).collect::<Vec<_>>()
                        ));
                        break;
                    }
                    let toggle = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text)
                            if [
                                "恢复证据与技术详情",
                                "错误详情与请求身份",
                                "最近查询的操作证据",
                                "预测依据与限制",
                                "原生字段",
                                "高级：原生字段编辑",
                                "依赖检查详情",
                            ]
                            .contains(&text.galley.text())
                                && !expanded.contains(text.galley.text()) =>
                        {
                            let rect = text
                                .galley
                                .rect
                                .translate(text.pos.to_vec2())
                                .intersect(shape.clip_rect);
                            (rect.height() > 8.0)
                                .then(|| (text.galley.text().to_string(), rect.center()))
                        }
                        _ => None,
                    });
                    if let Some((label, pos)) = toggle {
                        expanded.insert(label);
                        frame(
                            &mut app,
                            vec![
                                egui::Event::PointerMoved(pos),
                                egui::Event::PointerButton {
                                    pos,
                                    button: egui::PointerButton::Primary,
                                    pressed: true,
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ],
                        );
                        frame(
                            &mut app,
                            vec![egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: false,
                                modifiers: egui::Modifiers::NONE,
                            }],
                        );
                        for _ in 0..12 {
                            output = frame(&mut app, vec![]);
                        }
                        continue;
                    }
                    output = frame(
                        &mut app,
                        vec![
                            egui::Event::PointerMoved(egui::pos2(width - 30.0, height * 0.5)),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, -200.0),
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                }
                if appearance == Appearance::Light
                    && width == 760.0
                    && (page == Page::Diagnostics
                        || page == Page::Logs
                        || (page == Page::Profiles
                            && profile_tab == ProfileTab::Advanced
                            && advanced_tab == AdvancedTab::Json)
                        || (page == Page::Settings
                            && matches!(settings_tab, SettingsTab::Core | SettingsTab::Storage)))
                {
                    capture_layout(
                        &ctx,
                        output,
                        &format!("audit-{page:?}-{profile_tab:?}-{settings_tab:?}"),
                        width,
                        height,
                    );
                }
                if page == Page::Profiles
                    && (matches!(profile_tab, ProfileTab::Outbounds | ProfileTab::Routing)
                        || (profile_tab == ProfileTab::Advanced
                            && advanced_tab == AdvancedTab::Inbounds))
                {
                    assert!(
                        expanded.contains(if profile_tab == ProfileTab::Advanced {
                            "原生字段"
                        } else {
                            "高级：原生字段编辑"
                        }),
                        "native fields were not exercised: {appearance:?} {width} {page:?}/{profile_tab:?} expanded={expanded:?}"
                    );
                }
                if page == Page::Diagnostics {
                    assert!(expanded.contains("最近查询的操作证据"));
                }
                if page == Page::Settings && settings_tab == SettingsTab::Core {
                    assert!(expanded.contains("依赖检查详情"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn long_feedback_preserves_the_workspace_without_a_full_message_button() {
    let ctx = egui::Context::default();
    configure(&ctx);
    let mut app = app();
    app.message = "长状态消息不应挤占工作区\n".repeat(80);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(760.0, 500.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    let mut output = frame(&mut app, vec![]);
    for _ in 0..12 {
        output = frame(&mut app, vec![]);
    }
    let heading = text_bounds(&output, "Core 运行中");
    assert!(
        heading.top() > 60.0 && heading.bottom() < 400.0,
        "feedback consumed workspace: {heading:?}"
    );
    assert!(!output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "完整消息")
    }));
    capture_layout(&ctx, output, "audit-long-feedback", 760.0, 500.0);
}

#[test]
fn every_confirmation_dialog_keeps_actions_visible_with_long_native_names() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for (width, height) in [(760.0, 500.0), (1100.0, 760.0), (1920.0, 1040.0)] {
            let actions = [
                Action::Switch("work".into()),
                Action::Discard("work".into()),
                Action::Delete {
                    profile: "work".into(),
                    revision: "revision".into(),
                },
                Action::Stop,
                Action::Restart,
                Action::Proxy(ProxyMode::Global),
                Action::Proxy(ProxyMode::Direct),
                Action::AdoptDraft("work".into()),
            ];
            for action in actions {
                let ctx = egui::Context::default();
                configure(&ctx);
                appearance.apply(&ctx);
                let mut app = app();
                app.ui.runtime = None;
                app.status
                    .as_mut()
                    .unwrap()
                    .generation
                    .as_mut()
                    .unwrap()
                    .http_inbound_tag = "long-native-tag-".repeat(150);
                app.ui
                    .workspace
                    .names
                    .insert("work".into(), "长配置名称".repeat(16));
                let (title, _, button) = action.copy();
                app.ui.dialog = Some(Dialog::Confirm(action));
                let mut output = None;
                for _ in 0..12 {
                    output = Some(ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, height),
                            )),
                            ..Default::default()
                        },
                        |ctx| app.render(ctx),
                    ));
                }
                let output = output.unwrap();
                for label in [title, "取消", button] {
                    let rect = text_bounds(&output, label);
                    let clip = output
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Text(t) if t.galley.text() == label => Some(s.clip_rect),
                            _ => None,
                        })
                        .unwrap();
                    assert!(
                        rect.top() > 0.0 && rect.bottom() < height && clip.contains_rect(rect),
                        "clipped {label}"
                    );
                }
                if button == "确认接管" && width == 760.0 {
                    capture_layout(
                        &ctx,
                        output,
                        &format!("{appearance:?}-760-audit-long-confirmation"),
                        width,
                        height,
                    );
                }
            }
        }
    }
}

fn block_draft_writes(app: &mut App) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("config")).unwrap();
    std::fs::write(root.path().join("config/editor-drafts"), "blocked").unwrap();
    app.root = root.path().into();
    app.ui.persistence = true;
    app.editor = "{\"unsaved\":true}".into();
    app.draft_edited();
    root
}

#[test]
fn all_normal_exit_entries_block_on_draft_failure_and_allow_retry_or_explicit_discard() {
    for entry in ["tray-gui", "tray-quit", "window"] {
        for discard in [false, true] {
            let mut app = app();
            let root = block_draft_writes(&mut app);
            let ctx = egui::Context::default();
            match entry {
                "tray-gui" => app.tray_action("exit-gui", &ctx),
                "tray-quit" => app.tray_action("quit", &ctx),
                _ => app.request_exit(ExitKind::Gui),
            }
            app.advance_shutdown();
            assert!(!app.exit, "{entry}");
            assert!(!app.shutdown_requested, "{entry}");
            assert!(
                matches!(app.ui.dialog, Some(Dialog::UnsavedExit(_))),
                "{entry}"
            );
            assert!(app.ui.draft_dirty_since.is_some());
            assert!(app.ui.pending_draft_writes.contains("work"));
            let export = tempfile::tempdir().unwrap();
            assert!(app.export_unsaved_drafts(&export.path().join("drafts.json")));
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(export.path().join("drafts.json")).unwrap())
                    .unwrap();
            assert_eq!(value["drafts"]["work"]["json"], app.editor);
            assert!(!app.exit, "export alone must not exit");
            let kind = if entry.ends_with("quit") {
                ExitKind::Shutdown
            } else {
                ExitKind::Gui
            };
            if discard {
                app.discard_unsaved_and_exit(kind);
            } else {
                std::fs::remove_file(root.path().join("config/editor-drafts")).unwrap();
                app.request_exit(kind);
                app.advance_shutdown();
                assert!(app.ui.pending_draft_writes.is_empty());
                assert!(app.ui.draft_dirty_since.is_none());
                assert_eq!(
                    Draft::load(root.path(), "work").unwrap().unwrap().json,
                    app.editor
                );
                assert!(app.ui.failure.is_none());
            }
            if matches!(kind, ExitKind::Gui) {
                assert!(app.exit);
            } else {
                assert!(app.shutdown_requested);
            }
        }
    }
}

#[test]
fn background_draft_write_failures_are_retried_even_after_switching_profiles() {
    let mut app = app();
    let root = tempfile::tempdir().unwrap();
    app.root = root.path().into();
    app.ui.persistence = true;
    app.editor = "{\"s\":2}".into();
    assert!(app.stash_draft());
    app.load(Profile {
        id: "other".into(),
        json: "{}".into(),
        revision: hash(b"{}"),
    });
    let path = root.path().join("config/editor-drafts/work.json");
    let before = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    app.applied_draft("work", Some("{\"s\":1}"));
    assert!(app.ui.pending_draft_writes.contains("work"));
    assert!(app.ui.draft_dirty_since.is_some());
    assert!(!app.drafts_ready_for_exit(ExitKind::Gui));
    assert_eq!(app.ui.drafts["work"].json, "{\"s\":2}");
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, before).unwrap();
    app.ui.draft_dirty_since = Some(Instant::now() - Duration::from_secs(2));
    app.flush_draft_if_due();
    assert!(app.ui.pending_draft_writes.is_empty());
    assert!(app.ui.failure.is_none());
    assert!(app.ui.dialog.is_none());
    assert!(app.message.is_empty());
    assert_eq!(
        Draft::load(root.path(), "work")
            .unwrap()
            .unwrap()
            .base_revision,
        hash(b"{\"s\":1}")
    );
}

#[test]
fn explicit_discard_does_not_authorize_later_edits_to_be_lost() {
    let mut app = app();
    let _root = block_draft_writes(&mut app);
    assert!(!app.stash_draft());
    app.discard_unsaved_and_exit(ExitKind::Shutdown);
    app.editor = "{\"newer\":true}".into();
    app.draft_edited();
    assert!(!app.drafts_ready_for_exit(ExitKind::Gui));
    assert!(!app.exit);
}

fn guided_frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    width: f32,
    height: f32,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            events,
            ..Default::default()
        },
        |ctx| app.render(ctx),
    )
}
fn guided_click(app: &mut App, ctx: &egui::Context, label: &str, width: f32, height: f32) {
    let mut output = guided_frame(app, ctx, vec![], width, height);
    for _ in 0..12 {
        output = guided_frame(app, ctx, vec![], width, height);
    }
    let pos = text_bounds(&output, label).center();
    guided_frame(
        app,
        ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        width,
        height,
    );
    guided_frame(
        app,
        ctx,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        width,
        height,
    );
}

#[test]
fn new_wizard_keeps_actions_visible_and_only_saves_incomplete_native_profiles() {
    use crate::guided_config::{Scene, Template};
    for appearance in [Appearance::Light, Appearance::Dark] {
        for (width, height) in [(760.0, 500.0), (1100.0, 760.0)] {
            for template in Template::ALL {
                let mut app = app();
                let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
                app.tx = tx;
                app.pending.clear();
                app.begin_create(false, false);
                app.ui.new_name = "手工节点".into();
                app.ui.new_scene = Scene::Split;
                app.ui.new_template = template;
                let ctx = egui::Context::default();
                configure(&ctx);
                appearance.apply(&ctx);
                let mut output = guided_frame(&mut app, &ctx, vec![], width, height);
                for _ in 0..12 {
                    output = guided_frame(&mut app, &ctx, vec![], width, height);
                }
                let viewport =
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
                for label in ["新建配置", "取消", "保存待补充配置"] {
                    assert!(
                        viewport.contains_rect(text_bounds(&output, label)),
                        "{template:?} {label}"
                    );
                }
                if template == Template::VlessReality {
                    capture_layout(
                        &ctx,
                        output,
                        &format!("{appearance:?}-{width}-guided-create"),
                        width,
                        height,
                    );
                }
                guided_click(&mut app, &ctx, "保存待补充配置", width, height);
                assert!(app.ui.dialog.is_none());
                let request = requests.try_recv().unwrap();
                let Command::CreateProfile { profile_id, json } = request.command else {
                    panic!("wizard must only create a profile")
                };
                let value: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(value["outbounds"][0]["tag"], "node-1");
                assert!(
                    !value["routing"]["rules"].as_array().unwrap().is_empty(),
                    "split preset should populate default bypass rules"
                );
                assert_eq!(value["inbounds"][0]["sniffing"]["enabled"], true);
                assert!(requests.try_recv().is_err());
                assert!(app.ui.workspace.node_names.contains_key(&profile_id));
                assert_eq!(app.mode, ProxyMode::Direct);
            }
        }
    }
}

#[test]
fn issue_action_opens_the_node_form_and_edits_only_the_draft() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.pending.clear();
    let mut config: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    config["outbounds"][0] = serde_json::json!({"tag":"node-1","protocol":"socks","settings":{"servers":[{"port":0,"custom":"preserved"}]},"extension":{"keep":true}});
    app.editor = serde_json::to_string_pretty(&config).unwrap();
    app.ui.page = Page::Profiles;
    app.ui.profile_tab = ProfileTab::Outbounds;
    app.ui.guided.native = true;
    let original = app.editor.clone();
    let runtime = app.ui.runtime.as_ref().unwrap().json.clone();
    let ctx = egui::Context::default();
    configure(&ctx);
    guided_click(&mut app, &ctx, "查看 2 项补充 / 核对提示", 1100.0, 760.0);
    guided_click(&mut app, &ctx, "去补充", 1100.0, 760.0);
    assert_eq!(app.ui.profile_tab, ProfileTab::Outbounds);
    for _ in 0..12 {
        guided_frame(&mut app, &ctx, vec![], 1100.0, 760.0);
    }
    assert_eq!(app.editor, original);
    guided_frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("server.invalid".into())],
        1100.0,
        760.0,
    );
    let edited: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    assert_eq!(
        edited["outbounds"][0]["settings"]["servers"][0]["address"],
        "server.invalid"
    );
    assert_eq!(
        edited["outbounds"][0]["extension"],
        config["outbounds"][0]["extension"]
    );
    assert_eq!(
        edited["outbounds"][0]["settings"]["servers"][0]["custom"],
        "preserved"
    );
    assert_eq!(app.ui.runtime.as_ref().unwrap().json, runtime);
    assert!(requests.try_recv().is_err());
    assert!(
        !proxy_config::inspect_config(&edited)
            .iter()
            .any(|i| i.path.ends_with("/address"))
    );
}

#[test]
fn a_new_wizard_hides_authentication_even_if_the_previous_wizard_revealed_it() {
    use crate::guided_config::{Scene, Template};
    let mut app = app();
    let ctx = egui::Context::default();
    configure(&ctx);
    let prepare = |app: &mut App| {
        app.begin_create(false, false);
        app.ui.new_template = Template::VlessTls;
        app.ui.new_config_key = Some((
            Scene::Single,
            Template::VlessTls,
            app.ui.new_split_preset,
            app.ui.new_split_sniffing,
        ));
        let mut value = crate::guided_config::scene(Scene::Single, Template::VlessTls);
        value["outbounds"][0]["settings"]["vnext"][0]["users"][0]["id"] =
            serde_json::json!("test-credential-only");
        app.ui.new_json = serde_json::to_string(&value).unwrap();
    };
    prepare(&mut app);
    guided_click(&mut app, &ctx, "显示认证信息", 1100.0, 1040.0);
    let output = guided_frame(&mut app, &ctx, vec![], 1100.0, 1040.0);
    assert!(output.shapes.iter().any(
        |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="test-credential-only")
    ));
    prepare(&mut app);
    let output = guided_frame(&mut app, &ctx, vec![], 1100.0, 1040.0);
    assert!(!output.shapes.iter().any(
        |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="test-credential-only")
    ));
}

#[test]
fn guided_node_and_routing_forms_fit_the_parent_without_overlapping_text() {
    use crate::guided_config::{Scene, Template};
    for appearance in [Appearance::Light, Appearance::Dark] {
        for (width, height) in [(760.0, 500.0), (1100.0, 760.0), (1920.0, 1040.0)] {
            for tab in [ProfileTab::Outbounds, ProfileTab::Routing] {
                let mut app = app();
                let mut value = crate::guided_config::scene(Scene::Split, Template::Socks);
                value["outbounds"][0]["settings"]["servers"][0] =
                    serde_json::json!({"address":"proxy.example.com","port":443});
                value["routing"]["rules"] = serde_json::json!([
                    {"type":"field","domain":["domain:example.com","full:only.example.org"],"outboundTag":"node-1"},
                    {"type":"field","ip":["192.0.2.0/24"],"outboundTag":"direct"}
                ]);
                app.editor = serde_json::to_string_pretty(&value).unwrap();
                app.ui.page = Page::Profiles;
                app.ui.profile_tab = tab;
                app.ui.guided.focus = Some("/outbounds/0/settings/servers/0/address".into());
                app.ui.workspace.node_names.insert(
                    "work".into(),
                    std::collections::BTreeMap::from([("node-1".into(), "香港节点".into())]),
                );
                let original = app.editor.clone();
                let ctx = egui::Context::default();
                configure(&ctx);
                appearance.apply(&ctx);
                let mut output = guided_frame(&mut app, &ctx, vec![], width, height);
                for _ in 0..12 {
                    output = guided_frame(&mut app, &ctx, vec![], width, height);
                }
                assert!(
                    overlapping_visible_text(&output).is_empty(),
                    "{appearance:?} {width} {tab:?}: {:?}",
                    overlapping_visible_text(&output)
                );
                assert_eq!(app.editor, original);
                if tab == ProfileTab::Routing && width >= 1100.0 {
                    let first_match = text_bounds(&output, "example.com\nfull:only.example.org");
                    let footer = text_bounds(&output, "检查并应用");
                    assert!(
                        first_match.bottom() < footer.top(),
                        "first rule input must be visible: {first_match:?} / {footer:?}"
                    );
                }
                capture_layout(
                    &ctx,
                    output,
                    &format!("{appearance:?}-{width}-guided-{tab:?}"),
                    width,
                    height,
                );
            }
        }
    }
}

#[test]
fn unfinished_number_notice_locates_its_field_without_losing_input_or_overlapping_controls() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for (width, height) in [(760.0, 500.0), (1100.0, 760.0), (1920.0, 1040.0)] {
            let mut app = app();
            app.ui.page = Page::Profiles;
            app.ui.profile_tab = ProfileTab::Advanced;
            app.ui.advanced_tab = AdvancedTab::Json;
            let path = "/inbounds/0/port";
            let source = serde_json::from_str::<serde_json::Value>(&app.editor)
                .unwrap()
                .pointer(path)
                .unwrap()
                .to_string();
            app.ui.number_inputs.insert(
                path.into(),
                crate::workspace::NumberInput {
                    source,
                    text: "-".into(),
                },
            );
            app.draft_edited();
            let before = app.editor.clone();
            let ctx = egui::Context::default();
            configure(&ctx);
            appearance.apply(&ctx);
            for _ in 0..4 {
                guided_frame(&mut app, &ctx, vec![], width, height);
            }
            // The smallest window stacks the picker above the editor; scroll the page to its body.
            if width < 1100.0 {
                guided_frame(
                    &mut app,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(width - 80.0, height - 100.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -240.0),
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    width,
                    height,
                );
                for _ in 0..12 {
                    guided_frame(&mut app, &ctx, vec![], width, height);
                }
            }
            guided_click(&mut app, &ctx, "定位未完成数值", width, height);
            assert_eq!(app.ui.profile_tab, ProfileTab::Advanced);
            assert_eq!(app.ui.advanced_tab, AdvancedTab::Inbounds);
            assert!(app.ui.guided.native);
            let mut output = guided_frame(&mut app, &ctx, vec![], width, height);
            for _ in 0..4 {
                output = guided_frame(&mut app, &ctx, vec![], width, height);
            }
            assert!(
                overlapping_visible_text(&output).is_empty(),
                "{appearance:?} {width}: {:?}",
                overlapping_visible_text(&output)
            );
            assert_eq!(app.editor, before);
            assert_eq!(app.ui.number_inputs[path].text, "-");
        }
    }
}

#[test]
fn pac_draft_is_per_profile_persisted_and_can_be_tested() {
    let mut app = app();
    app.ui.page = Page::Pac;
    let mut draft = app.pac_draft();
    assert_eq!(draft.port, proxy_model::DEFAULT_PAC_PORT);
    assert_eq!(draft.mode, proxy_model::PacMode::AutoRules);
    draft.mode = proxy_model::PacMode::Custom;
    draft.script = "function FindProxyForURL(){ return 'DIRECT'; }".into();
    draft.port = 18081;
    app.save_pac_draft(draft.clone());
    assert_eq!(app.pac_draft().script, draft.script);
    assert_eq!(app.pac_draft().port, 18081);
    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.connected = true;
    render(&mut app, &ctx, 1100.0, 760.0);
}

#[test]
fn structured_edits_can_be_undone_and_redone() {
    let mut app = app();
    let original = app.editor.clone();
    let edited = original.replace("\"loglevel\": \"warning\"", "\"loglevel\": \"info\"");
    assert_ne!(edited, original);
    app.editor = edited.clone();
    app.draft_edited();
    assert_eq!(app.ui.undo.len(), 1);
    app.undo_edit();
    assert_eq!(app.editor, original);
    app.redo_edit();
    assert_eq!(app.editor, edited);
}

#[test]
fn proxy_mode_switch_uses_the_running_profile_pac_draft() {
    let mut app = app();
    app.ui.workspace.pac.insert(
        "work".into(),
        PacDraft {
            mode: proxy_model::PacMode::Custom,
            script: "function FindProxyForURL(){ return 'DIRECT'; }".into(),
            port: 18080,
            ..Default::default()
        },
    );
    app.ui.workspace.pac.insert(
        "other".into(),
        PacDraft {
            mode: proxy_model::PacMode::Custom,
            script: "function FindProxyForURL(){ return 'PROXY 127.0.0.1:1'; }".into(),
            port: 18081,
            ..Default::default()
        },
    );
    app.profile_id = "other".into();
    assert_eq!(app.pac_draft().port, 18081);
    assert_eq!(app.running_pac_draft().port, 18080);
}

#[test]
fn pac_endpoint_refreshes_after_config_adds_or_renames_an_http_inbound() {
    let mut app = app();
    app.http_tag = "http-in".into();
    app.editor = r#"{"outbounds":[{"tag":"direct","protocol":"freedom"}]}"#.into();
    assert!(app.pac_endpoint().is_err());
    app.editor = r#"{"inbounds":[{"tag":"new-http","listen":"127.0.0.1","port":18089,"protocol":"http"}],"outbounds":[{"tag":"direct","protocol":"freedom"}]}"#.into();
    let endpoint = app.pac_endpoint().unwrap();
    assert_eq!(endpoint.port(), 18089);
    assert_eq!(app.http_tag, "new-http");

    // Wildcard 0.0.0.0 inbound resolves to 127.0.0.1 for PAC target generation.
    app.editor = r#"{"inbounds":[{"tag":"all-http","listen":"0.0.0.0","port":18090,"protocol":"http"}],"outbounds":[{"tag":"direct","protocol":"freedom"}]}"#.into();
    let wildcard_endpoint = app.pac_endpoint().unwrap();
    assert_eq!(
        wildcard_endpoint,
        "127.0.0.1:18090".parse::<std::net::SocketAddr>().unwrap()
    );
    assert_eq!(app.http_tag, "all-http");
}

#[test]
fn pac_template_domain_list_is_saved_per_profile() {
    let mut app = app();
    let mut draft = app.pac_draft();
    assert_eq!(draft.mode, proxy_model::PacMode::AutoRules);
    draft.mode = proxy_model::PacMode::ProxyList;
    draft.geosites = vec!["geosite:google".into()];
    draft.domains = vec!["example.com".into(), "*.internal.test".into()];
    app.save_pac_draft(draft);
    let loaded = app.pac_draft();
    assert_eq!(loaded.mode, proxy_model::PacMode::ProxyList);
    assert_eq!(loaded.geosites, ["geosite:google"]);
    assert_eq!(loaded.domains, ["example.com", "*.internal.test"]);
}

#[test]
fn stopped_core_is_restarted_when_the_last_state_was_running() {
    let mut app = app();
    app.ui.connected = true;
    app.ui.settings_loaded = true;
    app.settings.restore_last_state = true;
    app.ui.workspace.last_core_running = true;
    if let Some(status) = app.status.as_mut() {
        status.core_state = "Stopped".into();
        status.recovery_required = false;
        if let Some(generation) = status.generation.as_mut() {
            generation.running = false;
        }
    }
    app.maybe_restore_core();
    assert!(app.ui.restore_attempted);
}

#[test]
fn explicit_stop_clears_the_restore_intent() {
    let mut app = app();
    app.ui.connected = true;
    app.ui.workspace.last_core_running = true;
    app.send(Command::StopCore);
    assert!(!app.ui.workspace.last_core_running);
}

#[test]
fn pac_proxy_mode_is_persisted_across_shutdown_and_restored_on_startup() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    let (_responses, rx) = mpsc::channel();
    let mut app = App::with_channels(dir.path().to_path_buf(), tx, rx);
    app.ui.persistence = true;
    app.ui.connected = true;
    app.ui.settings_loaded = true;
    app.settings.restore_last_state = true;
    let json = default_config();
    let revision = hash(json.as_bytes());
    let applied_pac = proxy_model::PacDraft {
        mode: proxy_model::PacMode::ProxyList,
        geosites: vec!["geosite:google".into()],
        domains: vec!["example.com".into()],
        script: String::new(),
        port: 17890,
    };
    app.note_applied_proxy(ProxyMode::Pac, Some(&applied_pac));
    app.status = Some(Status {
        generation: Some(Generation {
            id: "generation-pac".into(),
            transaction_id: "tx-pac".into(),
            request_id: "req-pac".into(),
            candidate: Candidate {
                id: "cand-1".into(),
                digest: "digest-1".into(),
                profile_id: "work".into(),
                base_revision: revision.clone(),
                revision,
                config_path: "config.json".into(),
                core: CoreIdentity {
                    path: "frozen/core".into(),
                    sha256: "sha".into(),
                    version: "Xray test".into(),
                },
                resources: Resources {
                    id: "resources".into(),
                    root: "frozen/resources".into(),
                    files: vec![],
                    complete: true,
                    notes: vec![],
                    dynamic: vec![],
                },
            },
            proxy_mode: ProxyMode::Pac,
            http_inbound_tag: "http-in".into(),
            running: true,
            pac: None,
        }),
        core_state: "Running".into(),
        proxy_state: "ManagedPac".into(),
        runtime_instance_id: Some("instance".into()),
        supervisor_epoch: "epoch".into(),
        last_error: None,
        recovery_required: false,
        winhttp: None,
        listener_state: "Ready".into(),
        pac_state: "Ready".into(),
        auto_connect_notice: None,
        proxy_availability: None,
    });
    // Unapplied draft changes for the profile must not override the last applied PAC config.
    app.profile_id = "work".into();
    app.save_pac_draft(proxy_model::PacDraft {
        domains: vec!["unapplied.example".into()],
        ..Default::default()
    });
    app.request_exit(ExitKind::Shutdown);
    assert!(app.shutdown_requested);
    drop(requests);

    // Simulate next launch where stop_generation wrote proxy_mode: Direct, running: false.
    let (tx2, requests2) = mpsc::sync_channel(MAX_PENDING);
    let (_responses2, rx2) = mpsc::channel();
    let mut restored = App::with_channels(dir.path().to_path_buf(), tx2, rx2);
    restored.ui.persistence = true;
    restored.ui.workspace = crate::workspace::Workspace::load(dir.path()).unwrap();
    restored.ui.connected = true;
    restored.ui.settings_loaded = true;
    restored.settings.restore_last_state = true;
    let mut stopped = app.status.unwrap();
    stopped.core_state = "Stopped".into();
    stopped.proxy_state = "Released".into();
    stopped.pac_state = "Stopped".into();
    if let Some(generation) = stopped.generation.as_mut() {
        generation.proxy_mode = ProxyMode::Direct;
        generation.running = false;
        generation.pac = None;
    }
    restored.status = Some(stopped);
    // Simulate a status poll arriving with Stopped before restore fires; it must not clear intent.
    restored.sync_restore_state(false, false);
    restored.maybe_restore_core();
    assert!(restored.ui.restore_attempted);
    let request = requests2.try_recv().unwrap();
    match request.command {
        Command::SetProxyMode {
            mode,
            http_inbound_tag,
            pac,
        } => {
            assert_eq!(mode, ProxyMode::Pac);
            assert_eq!(http_inbound_tag, "http-in");
            assert_eq!(pac, Some(applied_pac));
        }
        other => panic!("expected SetProxyMode(Pac), got {other:?}"),
    }
}

#[test]
fn global_proxy_mode_is_restored_on_startup_while_manual_stopped_switch_stays_blocked() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.ui.connected = true;
    app.ui.settings_loaded = true;
    app.settings.restore_last_state = true;
    if let Some(status) = app.status.as_mut()
        && let Some(generation) = status.generation.as_mut()
    {
        generation.proxy_mode = ProxyMode::Global;
    }
    app.sync_restore_state(true, false);
    app.request_exit(ExitKind::Shutdown);

    app.shutdown_requested = false;
    if let Some(status) = app.status.as_mut() {
        status.core_state = "Stopped".into();
        if let Some(generation) = status.generation.as_mut() {
            generation.proxy_mode = ProxyMode::Direct;
            generation.running = false;
        }
    }
    // Manual SetProxyMode while Stopped remains blocked.
    app.send(Command::SetProxyMode {
        mode: ProxyMode::Global,
        http_inbound_tag: "http-in".into(),
        pac: None,
    });
    assert!(requests.try_recv().is_err());

    // Automatic startup restore dispatches SetProxyMode(Global) cleanly.
    app.maybe_restore_core();
    assert!(app.ui.restore_attempted);
    let request = requests.try_recv().unwrap();
    assert!(matches!(
        request.command,
        Command::SetProxyMode {
            mode: ProxyMode::Global,
            pac: None,
            ..
        }
    ));
}

#[test]
fn diagnostics_avoids_unverified_placeholders_and_shows_recovery_context() {
    let mut app = app();
    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.page = Page::Diagnostics;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 760.0),
            )),
            ..Default::default()
        },
        |ctx| app.render(ctx),
    );
    let labels: Vec<&str> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect();
    assert!(!labels.contains(&"真实网络连通性"));
    assert!(!labels.contains(&"实际出口"));
    assert!(labels.contains(&"恢复状态"));
    assert!(labels.contains(&"代理模式"));
}

#[test]
fn tray_exit_opens_one_confirmation_with_remembered_default() {
    let mut app = app();
    let ctx = egui::Context::default();
    app.ui.workspace.exit_default = crate::workspace::ExitDefault::Shutdown;
    app.tray_action("exit", &ctx);
    assert!(matches!(app.ui.dialog, Some(Dialog::ExitChoice)));
    assert_eq!(app.ui.exit_choice, ExitKind::Shutdown);
    assert!(app.ui.exit_remember);
}

#[test]
fn empty_pac_page_does_not_show_a_json_parse_error() {
    let mut app = app();
    app.status = None;
    app.ui.runtime = None;
    app.ui.profile_cache.clear();
    app.ui.drafts.clear();
    app.profile_id.clear();
    app.selected_profile.clear();
    app.editor.clear();
    app.ui.page = Page::Pac;
    let ctx = egui::Context::default();
    configure(&ctx);
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 760.0),
            )),
            ..Default::default()
        },
        |ctx| app.render(ctx),
    );
    let labels: Vec<&str> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect();
    assert!(!labels.iter().any(|label| label.contains("无法解析")));
    assert!(!labels.iter().any(|label| label.contains("EOF")));
    assert!(labels.iter().any(|label| label.contains("尚未选择配置")));
}

#[test]
fn onboarding_actions_share_one_row() {
    let mut app = app();
    app.ui.onboarding = true;
    app.ui.setup_step = 0;
    app.core_path = "C:\\Xray\\xray.exe".into();
    let ctx = egui::Context::default();
    configure(&ctx);
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 760.0),
            )),
            ..Default::default()
        },
        |ctx| app.render(ctx),
    );
    let save = text_bounds(&output, "下一步：添加配置");
    let later = text_bounds(&output, "稍后设置，进入总览");
    assert!((save.center().y - later.center().y).abs() < 4.0);
}

#[test]
fn selecting_proxy_list_mode_persists_direct_template() {
    let mut app = app();
    app.ui.page = Page::Pac;
    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    let first = frame(&mut app, vec![]);
    let mode = text_bounds(&first, "按当前路由规则自动生成（推荐）").center();
    let click = |pos| {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    frame(&mut app, click(mode));
    let opened = frame(&mut app, vec![]);
    let choice = text_bounds(&opened, "仅代理选中 GeoSite 与域名，其余直连").center();
    frame(&mut app, click(choice));
    frame(&mut app, vec![]);
    let draft = app.pac_draft();
    assert_eq!(draft.mode, proxy_model::PacMode::ProxyList);
    assert!(draft.domains.is_empty());
    assert!(draft.script.contains("return \"DIRECT\""));
    assert!(app.ui.pac_checked);
}

#[test]
fn adding_a_domain_compiles_and_saves_proxy_list_script() {
    let mut app = app();
    app.ui.page = Page::Pac;
    let mut draft = app.pac_draft();
    draft.mode = proxy_model::PacMode::ProxyList;
    draft.script = "function FindProxyForURL(){ return 'PROXY 127.0.0.1:1'; }".into();
    app.save_pac_draft(draft);
    app.ui.pac_domain_input = "example.com".into();
    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    let output = frame(&mut app, vec![]);
    let add = text_bounds(&output, "添加").center();
    frame(&mut app, pointer_click(add));
    frame(&mut app, vec![]);
    let saved = app.pac_draft();
    assert_eq!(saved.mode, proxy_model::PacMode::ProxyList);
    assert_eq!(saved.domains, ["example.com"]);
    assert!(saved.script.contains(r#"return "PROXY"#));
    assert!(saved.script.contains(r#"return "DIRECT""#));
    assert!(app.ui.pac_checked);
}

#[test]
fn pac_page_shows_streamlined_sections_and_geosite_chips() {
    let asset_dir = tempfile::tempdir().unwrap();
    let geosite_bytes = proxy_config::geodata::test_encoder::encode_geosite_list(&[
        ("cn", &[(2, "baidu.com", &[][..])][..]),
        ("google", &[(2, "google.com", &[][..])][..]),
    ]);
    std::fs::write(asset_dir.path().join("geosite.dat"), geosite_bytes).unwrap();

    let mut app = app();
    app.asset_root = asset_dir.path().display().to_string();
    app.ui.page = Page::Pac;
    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 1200.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    // Default AutoRules mode hides Card 2 custom list for simplicity.
    let output = frame(&mut app, vec![]);
    let labels: Vec<&str> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect();
    assert!(labels.contains(&"1 · 分流模式与 GeoSite"));
    assert!(labels.contains(&"测试与一键应用"));
    assert!(!labels.contains(&"2 · 自定义域名清单"));
    assert!(labels.contains(&"检查并应用 PAC"));

    // Switching to DirectList shows Card 2 & quick GeoSite chips from geosite.dat!
    let mut draft = app.pac_draft();
    draft.mode = proxy_model::PacMode::DirectList;
    app.save_pac_draft(draft);
    let list_output = frame(&mut app, vec![]);
    let chip = text_bounds(&list_output, "国内常见 (cn)").center();
    frame(&mut app, pointer_click(chip));
    frame(&mut app, vec![]);
    let updated = app.pac_draft();
    assert_eq!(updated.geosites, vec!["geosite:cn"]);
    assert_eq!(
        proxy_pac::evaluate(&updated.script, "https://www.baidu.com/")
            .unwrap()
            .kind,
        "DIRECT"
    );
    assert_eq!(
        proxy_pac::evaluate(&updated.script, "https://example.org/")
            .unwrap()
            .kind,
        "PROXY"
    );
}

#[test]
fn history_diff_dialog_loads_only_after_confirmation() {
    let mut app = app();
    let before = app.editor.clone();
    let historical = before.replace("\"loglevel\": \"warning\"", "\"loglevel\": \"info\"");
    app.ui.history_preview = Some(HistoryPreview {
        json: historical.clone(),
        core: "Xray historical".into(),
    });
    app.ui.dialog = Some(Dialog::HistoryDiff);
    let ctx = egui::Context::default();
    configure(&ctx);
    let _output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 760.0),
            )),
            ..Default::default()
        },
        |ctx| app.render(ctx),
    );
    for _ in 0..3 {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 760.0),
                )),
                ..Default::default()
            },
            |ctx| app.render(ctx),
        );
    }
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 760.0),
            )),
            ..Default::default()
        },
        |ctx| app.render(ctx),
    );
    assert!(text_bounds(&output, "比较历史版本与当前草稿").top() > 0.0);
    let load = text_bounds(&output, "载入历史为草稿").center();
    assert_eq!(app.editor, before);
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 760.0),
            )),
            events: vec![
                egui::Event::PointerMoved(load),
                egui::Event::PointerButton {
                    pos: load,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: load,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        },
        |ctx| app.render(ctx),
    );
    let _ = output;
    assert_eq!(app.editor, historical);
    assert!(app.ui.history_preview.is_none());
}

fn pointer_click(pos: egui::Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

#[test]
fn pac_batch_dialog_imports_and_replaces_domains_with_error_protection() {
    let mut app = app();
    app.ui.page = Page::Pac;
    let mut draft = app.pac_draft();
    draft.mode = proxy_model::PacMode::ProxyList;
    app.save_pac_draft(draft);
    app.ui.dialog = Some(Dialog::PacBatch);
    app.ui.pac_batch_input =
        "# comment\nexample.com, *.sub.example.net\nfull:api.example.org\n".into();
    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    for _ in 0..3 {
        let _ = frame(&mut app, vec![]);
    }
    let output = frame(&mut app, vec![]);
    let import = text_bounds(&output, "导入并保存").center();
    frame(&mut app, pointer_click(import));
    frame(&mut app, vec![]);
    assert!(app.ui.dialog.is_none());
    let draft = app.pac_draft();
    assert_eq!(
        draft.domains,
        vec!["example.com", "*.sub.example.net", "full:api.example.org"]
    );
    assert_eq!(draft.mode, proxy_model::PacMode::ProxyList);

    // Invalid-only replacement preserves existing domains and reports error.
    assert!(!app.apply_pac_batch("bad/domain", true));
    assert_eq!(app.pac_draft().domains.len(), 3);
    assert!(app.ui.pac_batch_report.is_some());

    // Valid replacement updates the list cleanly.
    assert!(app.apply_pac_batch("only.example.org", true));
    assert_eq!(app.pac_draft().domains, vec!["only.example.org"]);
}

#[test]
fn pac_large_domain_list_uses_bounded_virtualized_viewport_and_filters() {
    let mut app = app();
    app.ui.page = Page::Pac;
    let mut draft = app.pac_draft();
    draft.mode = proxy_model::PacMode::ProxyList;
    draft.domains = (0..2000)
        .map(|i| match i % 3 {
            0 => format!("site{i}.example.com"),
            1 => format!("*.wild{i}.example.net"),
            _ => format!("full:exact{i}.example.org"),
        })
        .collect();
    app.save_pac_draft(draft);
    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 1400.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    let output = frame(&mut app, vec![]);
    let labels: Vec<&str> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect();
    assert!(labels.iter().any(|l| l.contains("共 2000 条")));
    // Virtualized scroll area renders only the visible rows, not all 2000 items.
    let rendered_domains = labels
        .iter()
        .filter(|l| {
            l.ends_with(".example.com")
                || l.ends_with(".example.net")
                || l.ends_with(".example.org")
        })
        .count();
    assert!(
        rendered_domains > 0 && rendered_domains < 40,
        "expected virtualized slice, got {rendered_domains}"
    );
    // Bounded scroll area keeps apply card reachable without stretching the page.
    let apply_section = text_bounds(&output, "测试与一键应用");
    assert!(
        apply_section.top() < 1300.0,
        "apply section pushed too far down: {}",
        apply_section.top()
    );

    // Search + kind filter + bulk selection removal.
    app.ui.pac_domain_filter = "exact199".into();
    app.ui.pac_domain_kind_filter = PacDomainFilter::Exact;
    let filtered = frame(&mut app, vec![]);
    let select_all = text_bounds(&filtered, "全选当前").center();
    frame(&mut app, pointer_click(select_all));
    let after_select = frame(&mut app, vec![]);
    let selected_count = app.ui.pac_selected_domains.len();
    assert!(selected_count > 0 && selected_count < 20);
    let remove_btn_label = format!("移除所选 ({selected_count})");
    let remove_pos = text_bounds(&after_select, &remove_btn_label).center();
    frame(&mut app, pointer_click(remove_pos));
    frame(&mut app, vec![]);
    assert_eq!(app.pac_draft().domains.len(), 2000 - selected_count);
    assert!(app.ui.pac_selected_domains.is_empty());
}

#[test]
fn pac_extract_from_config_imports_both_geosite_and_domains() {
    let mut app = app();
    app.ui.page = Page::Pac;
    let mut draft = app.pac_draft();
    draft.mode = proxy_model::PacMode::DirectList;
    app.save_pac_draft(draft);
    app.editor = r#"{
        "inbounds": [{"tag": "http-in", "listen": "127.0.0.1", "port": 10809, "protocol": "http"}],
        "outbounds": [{"tag": "direct", "protocol": "freedom"}],
        "routing": {"rules": [
            {"type": "field", "domain": ["geosite:cn", "domain:example.com", "full:api.example.org"], "outboundTag": "direct"}
        ]}
    }"#
    .into();
    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    let output = frame(&mut app, vec![]);
    let extract = text_bounds(&output, "从路由规则提取").center();
    frame(&mut app, pointer_click(extract));
    frame(&mut app, vec![]);
    let draft = app.pac_draft();
    assert_eq!(draft.geosites, vec!["geosite:cn"]);
    assert_eq!(draft.domains, vec!["example.com", "full:api.example.org"]);
}

#[test]
fn pac_clear_all_requires_confirmation_and_empties_list() {
    let mut app = app();
    app.ui.page = Page::Pac;
    let mut draft = app.pac_draft();
    draft.mode = proxy_model::PacMode::ProxyList;
    draft.domains = vec!["a.example.com".into(), "b.example.com".into()];
    app.save_pac_draft(draft);
    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 1200.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    let output = frame(&mut app, vec![]);
    let clear = text_bounds(&output, "清空清单").center();
    frame(&mut app, pointer_click(clear));
    for _ in 0..3 {
        let _ = frame(&mut app, vec![]);
    }
    let confirm_dialog = frame(&mut app, vec![]);
    assert!(matches!(
        app.ui.dialog,
        Some(Dialog::Confirm(Action::ClearPacDomains(_)))
    ));
    let confirm = text_bounds(&confirm_dialog, "确认清空").center();
    frame(&mut app, pointer_click(confirm));
    frame(&mut app, vec![]);
    assert!(app.pac_draft().domains.is_empty());
}

#[test]
fn logs_copy_buttons_and_session_log_export_helpers_work() {
    let dir = tempfile::tempdir().unwrap();
    let logs_dir = dir.path().join("logs");
    std::fs::create_dir_all(&logs_dir).unwrap();
    std::fs::write(logs_dir.join("supervisor.log.1"), "older line").unwrap();
    std::fs::write(logs_dir.join("supervisor.log"), "newer line\n").unwrap();

    let mut app = app();
    app.root = dir.path().into();
    app.logs = vec!["2026-11-13 [Info] core ready".into()];
    app.ui.page = Page::Logs;

    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    let copied_text = |output: &egui::FullOutput| -> Option<String> {
        output.platform_output.commands.iter().find_map(|cmd| {
            if let egui::OutputCommand::CopyText(text) = cmd {
                Some(text.clone())
            } else {
                None
            }
        })
    };

    let output = frame(&mut app, vec![]);
    let copy_filtered = text_bounds(&output, "复制筛选结果").center();
    let click_out = frame(&mut app, pointer_click(copy_filtered));
    assert_eq!(
        copied_text(&click_out).as_deref(),
        Some("[Core] 2026-11-13 [Info] core ready")
    );
    assert!(app.message.contains("已复制 1 条日志"));

    let output = frame(&mut app, vec![]);
    let copy_row = text_bounds(&output, "复制").center();
    let click_out = frame(&mut app, pointer_click(copy_row));
    assert_eq!(
        copied_text(&click_out).as_deref(),
        Some("[Core] 2026-11-13 [Info] core ready")
    );
    assert!(app.message.contains("已复制该条日志"));

    let output = frame(&mut app, vec![]);
    let copy_session = text_bounds(&output, "复制会话日志").center();
    let click_out = frame(&mut app, pointer_click(copy_session));
    assert_eq!(
        copied_text(&click_out).as_deref(),
        Some("older line\nnewer line\n")
    );
    assert!(app.message.contains("已复制持久化会话日志"));

    let exported = dir.path().join("exported.log");
    let content = app.read_session_logs().unwrap().unwrap();
    App::write_log_export(&exported, &content).unwrap();
    assert_eq!(
        std::fs::read_to_string(&exported).unwrap(),
        "older line\nnewer line\n"
    );
    assert!(App::write_log_export(dir.path(), &content).is_err());

    app.ui.page = Page::Settings;
    app.ui.settings_tab = SettingsTab::Storage;
    let output = frame(&mut app, vec![]);
    let copy_storage_log = text_bounds(&output, "复制日志内容").center();
    let click_out = frame(&mut app, pointer_click(copy_storage_log));
    assert_eq!(
        copied_text(&click_out).as_deref(),
        Some("older line\nnewer line\n")
    );
}

#[test]
fn proxy_lease_conflict_shows_confirmable_resolution_while_recovery_required_does_not() {
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    let (_responses, rx) = mpsc::channel();
    let mut base = app();
    let status = base.status.take().unwrap();
    let mut app = App::with_channels("unused-ui-test".into(), tx, rx);
    app.ui.connected = true;
    app.ui.setup_initialized = true;
    app.ui.onboarding = false;
    app.status = Some(Status {
        core_state: "Stopped".into(),
        proxy_state: "Conflict".into(),
        recovery_required: true,
        last_error: Some(Error::new(
            "PROXY_LEASE_CONFLICT",
            "External system proxy settings preserved",
        )),
        ..status
    });
    assert!(app.can_resolve_proxy_conflict());

    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };

    let output = frame(&mut app, vec![]);
    let resolve_btn = text_bounds(&output, "保留当前系统代理并解除冲突…").center();
    frame(&mut app, pointer_click(resolve_btn));
    for _ in 0..3 {
        let _ = frame(&mut app, vec![]);
    }
    let modal = frame(&mut app, vec![]);
    assert!(matches!(
        app.ui.dialog,
        Some(Dialog::Confirm(Action::ResolveProxyConflict))
    ));
    let confirm = text_bounds(&modal, "保留当前设置并解除").center();
    frame(&mut app, pointer_click(confirm));
    assert!(
        requests
            .try_iter()
            .any(|r| matches!(r.command, Command::ResolveProxyConflict))
    );
    app.busy = false;

    if let Some(status) = app.status.as_mut() {
        status.proxy_state = "RecoveryRequired".into();
        status.last_error = Some(Error::new("PROXY_RECOVERY_REQUIRED", "Broken lease"));
    }
    assert!(!app.can_resolve_proxy_conflict());
    assert!(app.can_save_settings());

    app.settings.start_on_logon = true;
    app.ui.page = Page::Settings;
    app.ui.settings_tab = SettingsTab::Startup;
    let output = frame(&mut app, vec![]);
    let toggle = text_bounds(&output, "登录 Windows 后自启（隐藏到系统托盘）").center();
    frame(&mut app, pointer_click(toggle));
    assert!(!app.settings.start_on_logon);
    let output = frame(&mut app, vec![]);
    let save = text_bounds(&output, "保存设置").center();
    frame(&mut app, pointer_click(save));
    assert!(requests.try_iter().any(|r| {
        matches!(
            r.command,
            Command::SaveSettings { ref settings } if !settings.start_on_logon
        )
    }));
    app.busy = false;

    if let Some(status) = app.status.as_mut() {
        status.last_error = Some(Error::new("STARTUP_CONFLICT", "External task changed"));
    }
    assert!(!app.can_save_settings());
}

#[test]
fn log_rows_show_local_timestamps_for_operations_and_localize_utc_prefixes() {
    let mut app = app();
    app.add_event("信息", "saved", "req-1");
    app.logs = vec!["[2026-11-14T04:04:50.5837401Z] core running".into()];

    let rows = app.log_rows();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].1.starts_with("[操作 / 信息] [20"));
    assert!(rows[0].1.contains("saved\nrequest_id: req-1"));
    assert!(rows[1].1.starts_with("[Core] [2026-11-"));
    assert!(!rows[1].1.contains("5837401Z]"));
}

#[test]
fn geodata_cache_supports_ext_without_geosite_and_reports_empty_geosite() {
    use proxy_config::geodata::test_encoder::encode_geosite_list;

    let asset_dir = tempfile::tempdir().unwrap();
    let custom_bytes = encode_geosite_list(&[("my_proxy_list", &[(2, "example.org", &[])])]);
    std::fs::write(asset_dir.path().join("custom_site.dat"), custom_bytes).unwrap();

    let mut app = app();
    app.asset_root = asset_dir.path().display().to_string();
    app.editor = r#"{"outbounds":[{"tag":"proxy","protocol":"vless"}]}"#.into();
    let cache = app.refresh_geodata_cache().unwrap();
    assert!(cache.geosite_error.is_none());
    assert!(
        cache
            .data
            .resolve_domain_spec("ext:custom_site.dat:my_proxy_list")
            .is_none()
    );

    // Adding an ext: reference to editor must invalidate cache even without geosite.dat.
    app.editor = r#"{
        "outbounds":[{"tag":"proxy","protocol":"vless"}],
        "routing":{"rules":[{"type":"field","domain":["ext:custom_site.dat:my_proxy_list"],"outboundTag":"proxy"}]}
    }"#
    .into();
    let cache = app.refresh_geodata_cache().unwrap();
    assert!(
        cache
            .data
            .resolve_domain_spec("ext:custom_site.dat:my_proxy_list")
            .is_some()
    );

    // Empty 0-byte geosite.dat surfaces an explicit error instead of pretending it's missing.
    std::fs::write(asset_dir.path().join("geosite.dat"), b"").unwrap();
    let cache = app.refresh_geodata_cache().unwrap();
    assert!(cache.geosite_error.is_some());
    assert!(
        cache
            .data
            .resolve_domain_spec("ext:custom_site.dat:my_proxy_list")
            .is_some()
    );
}

#[test]
fn portable_relative_paths_work_across_settings_workspace_and_geodata_cache() {
    use proxy_config::geodata::test_encoder::encode_geosite_list;

    let tmp = tempfile::tempdir().unwrap();
    let data_root = tmp.path().join("proxy-pilot").join("data");
    let dep_dir = data_root.join("dependencies/xray/v26");
    std::fs::create_dir_all(&dep_dir).unwrap();
    std::fs::write(
        dep_dir.join("geosite.dat"),
        encode_geosite_list(&[("cn", &[(2, "baidu.com", &[])])]),
    )
    .unwrap();

    let mut app = app();
    app.root = data_root.clone();
    app.core_path = dep_dir.join("xray").display().to_string();
    app.asset_root = dep_dir.display().to_string();

    let portable_core = proxy_model::to_portable_string(&app.root, &app.core_path);
    let portable_assets = proxy_model::to_portable_string(&app.root, &app.asset_root);
    app.core_path = portable_core.clone();
    app.asset_root = portable_assets.clone();
    app.settings.core_path = portable_core.clone().into();
    app.accept_saved_settings(app.settings.clone(), app.asset_root.clone());

    assert_eq!(app.core_path, "dependencies/xray/v26/xray");
    assert_eq!(app.asset_root, "dependencies/xray/v26");
    assert!(!app.ui.settings_dirty);
    assert_eq!(
        app.ui
            .workspace
            .asset_roots
            .get("dependencies/xray/v26/xray")
            .map(String::as_str),
        Some("dependencies/xray/v26")
    );

    let cache = app.refresh_geodata_cache().unwrap();
    assert!(cache.data.resolve_domain_spec("geosite:cn").is_some());
}

#[test]
fn share_link_import_works_in_create_dialog_and_nodes_tab_with_default_promotion() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.pending.clear();
    let ctx = egui::Context::default();
    configure(&ctx);

    // 1. Create dialog multi-line share link import without intermediate "开始填写".
    app.begin_create(false, false);
    app.ui.new_share_input = [
        "vless://11111111-2222-3333-4444-555555555555@jp.example.com:443?security=reality&sni=www.microsoft.com&fp=chrome&pbk=PK_TEST#Tokyo",
        "trojan://secret@hk.example.com:443?security=tls&sni=hk.example.com#HongKong",
    ]
    .join("\n");
    guided_click(&mut app, &ctx, "解析并填入", 1100.0, 760.0);
    assert!(app.ui.new_share_imported);
    assert_eq!(app.ui.new_name, "Tokyo");
    assert!(app.ui.new_share_error.is_none());
    let created: serde_json::Value = serde_json::from_str(&app.ui.new_json).unwrap();
    assert_eq!(
        created["outbounds"][0]["settings"]["vnext"][0]["address"],
        "jp.example.com"
    );
    assert_eq!(
        created["outbounds"][1]["settings"]["servers"][0]["address"],
        "hk.example.com"
    );

    guided_click(&mut app, &ctx, "添加配置", 1100.0, 760.0);
    assert!(app.ui.dialog.is_none());
    let req = requests.try_recv().unwrap();
    assert!(matches!(req.command, Command::CreateProfile { .. }));
    app.busy = false;

    // 2. Outbounds tab share link import + one-click "设为默认".
    app.ui.page = Page::Profiles;
    app.ui.profile_tab = ProfileTab::Outbounds;
    guided_click(&mut app, &ctx, "从分享链接导入节点…", 1100.0, 760.0);
    assert!(app.ui.node_share_open);
    app.ui.node_share_input =
        "ss://YWVzLTI1Ni1nY206cGFzczEyMw@sg.example.com:8388#Singapore".into();
    guided_click(&mut app, &ctx, "导入节点", 1100.0, 760.0);

    let edited: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    let outbounds = edited["outbounds"].as_array().unwrap();
    let sg_tag = outbounds.last().unwrap()["tag"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        app.ui.workspace.node_names["work"]
            .get(&sg_tag)
            .map(String::as_str),
        Some("Singapore")
    );
    assert_eq!(
        outbounds.last().unwrap()["settings"]["servers"][0]["address"],
        "sg.example.com"
    );
}

#[test]
fn active_pac_strategy_banner_shows_effective_mode_summary_and_draft_mismatch() {
    let mut app = app();
    let status = app.status.as_mut().unwrap();
    status.proxy_state = "Owned".into();
    status.pac_state = "Serving".into();
    let g = status.generation.as_mut().unwrap();
    g.proxy_mode = ProxyMode::Pac;
    g.pac = Some(PacPublication {
        revision: "active-pac-revision-00000000".into(),
        sha256: "active-pac-revision-00000000".into(),
        url: "http://127.0.0.1:18080/pac/active/proxy.pac".into(),
        port: 18080,
        path: PathBuf::from("runtime/pac/active/proxy.pac"),
        mode: Some(proxy_model::PacMode::ProxyList),
        summary: Some("1 个 GeoSite · 2 条代理域名（2 条生效），其余直连".into()),
    });
    app.ui.workspace.pac.insert(
        "work".into(),
        PacDraft {
            mode: proxy_model::PacMode::AutoRules,
            ..Default::default()
        },
    );

    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.page = Page::Pac;
    let output = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    let _ = text_bounds(&output, "生效中 · PAC 自动代理正在接管系统代理");
    let _ = text_bounds(
        &output,
        "仅代理选中 GeoSite 与域名，其余直连（1 个 GeoSite · 2 条代理域名（2 条生效），其余直连）",
    );
    let _ = text_bounds(
        &output,
        "注意：正在生效的 PAC 与本页当前编辑不一致；修改需点击下方「检查并应用 PAC」后才会生效。",
    );

    app.ui.page = Page::Overview;
    let overview = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    let _ = text_bounds(&overview, "生效 PAC 策略");
}

#[test]
fn pac_inputs_auto_select_all_on_focus_for_easy_replacement() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.pending.clear();
    app.ui.page = Page::Pac;
    assert_eq!(app.ui.pac_test_url, "https://example.com/");

    let ctx = egui::Context::default();
    configure(&ctx);
    let output = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    let url_pos = text_bounds(&output, "https://example.com/").center();
    guided_frame(&mut app, &ctx, pointer_click(url_pos), 1100.0, 900.0);
    guided_frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("https://google.com/".into())],
        1100.0,
        900.0,
    );
    assert_eq!(app.ui.pac_test_url, "https://google.com/");

    guided_click(&mut app, &ctx, "测试命中结果", 1100.0, 900.0);
    assert!(matches!(
        requests.try_recv(),
        Ok(Request {
            command: Command::TestPac { url, .. },
            ..
        }) if url == "https://google.com/"
    ));

    // Clicking "测试命中结果" re-focuses and selects all in the URL box so typing replaces it cleanly.
    guided_frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("https://github.com/".into())],
        1100.0,
        900.0,
    );
    assert_eq!(app.ui.pac_test_url, "https://github.com/");
}

#[test]
fn core_log_level_can_be_adjusted_from_logs_and_profile_settings() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.pending.clear();
    assert_eq!(app.core_log_level(), "warning");

    assert!(app.set_core_log_level("debug"));
    assert_eq!(app.core_log_level(), "debug");
    let edited: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    assert_eq!(edited["log"]["loglevel"], "debug");

    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.page = Page::Logs;
    let output = guided_frame(&mut app, &ctx, vec![], 1100.0, 760.0);
    assert!(overlapping_visible_text(&output).is_empty());
    let _ = text_bounds(&output, "Core 日志输出级别");
    let _ = text_bounds(&output, "运行中级别：warning");
    guided_click(&mut app, &ctx, "应用到 Core", 1100.0, 760.0);
    let req = requests.try_recv().unwrap();
    assert!(matches!(req.command, Command::ValidateDraft { .. }));
}

#[test]
fn route_prediction_input_retains_focus_and_selects_all_after_predict() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.pending.clear();
    app.ui.page = Page::Diagnostics;
    assert_eq!(app.route_host, "example.com");

    let ctx = egui::Context::default();
    configure(&ctx);
    let output = guided_frame(&mut app, &ctx, vec![], 1100.0, 1550.0);
    let host_pos = text_bounds(&output, "example.com").center();
    guided_frame(&mut app, &ctx, pointer_click(host_pos), 1100.0, 1550.0);
    guided_frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("google.com".into())],
        1100.0,
        1550.0,
    );
    assert_eq!(app.route_host, "google.com");

    guided_click(&mut app, &ctx, "预测命中规则与出站", 1100.0, 1550.0);
    assert!(matches!(
        requests.try_recv(),
        Ok(Request {
            command: Command::TestRunningRoute { context, .. },
            ..
        }) if context.host == "google.com"
    ));

    // Clicking "预测命中规则与出站" re-focuses and selects all in the host box for easy replacement.
    guided_frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("github.com".into())],
        1100.0,
        1550.0,
    );
    assert_eq!(app.route_host, "github.com");
}

#[test]
fn port_forward_tab_creates_links_routing_skips_sniffing_and_pauses_cleanly() {
    let mut app = app();
    app.ui.page = Page::Profiles;
    app.ui.profile_tab = ProfileTab::PortForward;

    let ctx = egui::Context::default();
    configure(&ctx);
    let output = guided_frame(&mut app, &ctx, vec![], 1100.0, 860.0);
    assert!(overlapping_visible_text(&output).is_empty());

    // Open Add Xray Port Forward modal, fill fields, and save.
    guided_click(&mut app, &ctx, "+ Xray 转发", 1100.0, 860.0);
    assert!(matches!(app.ui.dialog, Some(Dialog::PortForward)));
    app.ui.pf_form.name = "内网 MySQL".into();
    app.ui.pf_form.local_port = "13306".into();
    app.ui.pf_form.target_host = "10.0.0.5".into();
    app.ui.pf_form.target_port = "3306".into();
    app.ui.pf_form.outbound_tag = "direct".into();
    guided_click(&mut app, &ctx, "保存草稿", 1100.0, 860.0);
    assert!(app.ui.dialog.is_none());

    let parsed: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    assert_eq!(parsed["inbounds"][2]["protocol"], "dokodemo-door");
    assert_eq!(parsed["inbounds"][2]["port"], 13306);
    assert_eq!(parsed["routing"]["rules"][0]["inboundTag"][0], "pf-1");

    // Card renders cleanly at desktop and compact widths without text overlap.
    for width in [1100.0, 880.0] {
        let out = guided_frame(&mut app, &ctx, vec![], width, 860.0);
        assert!(overlapping_visible_text(&out).is_empty());
        let _ = text_bounds(&out, "内网 MySQL");
        let _ = text_bounds(&out, "待应用");
    }

    // Routing tab shows linkage card and "开启 Sniffing" skips dokodemo-door inbounds.
    app.ui.profile_tab = ProfileTab::Routing;
    let routing_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 860.0);
    assert!(overlapping_visible_text(&routing_out).is_empty());
    let _ = text_bounds(&routing_out, "前往端口转发");
    guided_click(&mut app, &ctx, "开启 Sniffing", 1100.0, 860.0);
    let after_sniff: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    assert_eq!(after_sniff["inbounds"][0]["sniffing"]["enabled"], true);
    assert!(after_sniff["inbounds"][2].get("sniffing").is_none());

    // Clicking "前往端口转发" navigates back to PortForward tab.
    guided_click(&mut app, &ctx, "前往端口转发", 1100.0, 860.0);
    assert_eq!(app.ui.profile_tab, ProfileTab::PortForward);
}

#[test]
fn exposed_port_forward_requires_preview_confirmation() {
    let mut app = app();
    let (tx, _requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    let mut value: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    let mut form = crate::port_forward::PortForwardForm::from_new(&value, &[]);
    form.listen = "0.0.0.0".into();
    form.local_port = "18080".into();
    form.target_host = "192.168.1.20".into();
    form.target_port = "80".into();
    crate::port_forward::save_entry(&mut value, &mut app.ui.workspace, &app.profile_id, &form)
        .unwrap();
    app.editor = serde_json::to_string_pretty(&value).unwrap();
    assert!(App::has_exposed_port_forward(&value));

    let candidate = Candidate {
        id: "cand-1".into(),
        digest: "dig-1".into(),
        profile_id: app.profile_id.clone(),
        base_revision: app.revision.clone(),
        revision: hash(app.editor.as_bytes()),
        config_path: "config.json".into(),
        core: CoreIdentity {
            path: "core".into(),
            sha256: "sha".into(),
            version: "Xray test".into(),
        },
        resources: Resources {
            id: "res-1".into(),
            root: "resources".into(),
            files: vec![],
            complete: true,
            notes: vec![],
            dynamic: vec![],
        },
    };
    app.accept_candidate_with_mode(candidate, true);
    assert!(app.ui.preview_open);
}

#[test]
fn port_forward_cross_page_parity_overview_nodes_json_and_diagnostics() {
    let mut app = app();
    let mut value: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    let mut form = crate::port_forward::PortForwardForm::from_new(&value, &[]);
    form.name = "内网数据库".into();
    form.local_port = "13306".into();
    form.target_host = "10.0.0.5".into();
    form.target_port = "3306".into();
    form.outbound_tag = "direct".into();
    crate::port_forward::save_entry(&mut value, &mut app.ui.workspace, &app.profile_id, &form)
        .unwrap();
    app.editor = serde_json::to_string_pretty(&value).unwrap();
    app.ui.runtime = Some(RuntimeConfig {
        generation_id: "generation-1".into(),
        json: app.editor.clone(),
    });

    let ctx = egui::Context::default();
    configure(&ctx);

    let has_text = |out: &egui::FullOutput, needle: &str| {
        out.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains(needle))
        })
    };

    // Overview lists active port forwards independently of Direct proxy mode.
    app.ui.page = Page::Overview;
    let overview_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 1200.0);
    assert!(overlapping_visible_text(&overview_out).is_empty());
    let _ = text_bounds(&overview_out, "端口转发与隧道");
    let _ = text_bounds(&overview_out, "内网数据库 (TCP)");

    // Profile list, PortForward tab badge, and Outbounds node card reference badge align cleanly.
    app.ui.page = Page::Profiles;
    app.ui.profile_tab = ProfileTab::Outbounds;
    let nodes_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 860.0);
    assert!(overlapping_visible_text(&nodes_out).is_empty());
    let _ = text_bounds(&nodes_out, "端口转发 (1)");
    assert!(has_text(&nodes_out, "被 1 条端口转发引用"));
    assert!(has_text(&nodes_out, "1 条转发"));

    // Diagnostics shows per-inbound probe detail and quick inboundTag fills forward target.
    app.ui.page = Page::Diagnostics;
    let diag_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 1200.0);
    assert!(overlapping_visible_text(&diag_out).is_empty());
    let _ = text_bounds(&diag_out, "TCP 监听就绪");
    guided_click(&mut app, &ctx, "pf-1", 1100.0, 1200.0);
    assert_eq!(app.route_inbound, "pf-1");
    assert_eq!(app.route_host, "10.0.0.5");
    assert_eq!(app.ui.route_port, "3306");
    assert_eq!(app.route_network, "tcp");

    // Pausing moves item to workspace paused list shown inside Advanced -> Json.
    let entries = crate::port_forward::extract_entries(&value, &app.ui.workspace, &app.profile_id);
    crate::port_forward::set_entry_enabled(
        &mut value,
        &mut app.ui.workspace,
        &app.profile_id,
        &entries[0],
        false,
    )
    .unwrap();
    app.editor = serde_json::to_string_pretty(&value).unwrap();
    app.ui.page = Page::Profiles;
    app.ui.profile_tab = ProfileTab::Advanced;
    app.ui.advanced_tab = AdvancedTab::Json;
    let json_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 860.0);
    assert!(overlapping_visible_text(&json_out).is_empty());
    let _ = text_bounds(
        &json_out,
        "工作区停用端口转发暂存（1 条 · 不写入 Core JSON）",
    );
}

#[test]
fn switching_profiles_preserves_per_profile_proxy_mode_and_pac_strategy() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    app.root = dir.path().to_path_buf();
    app.ui.persistence = true;
    app.ui.connected = false;
    app.profiles = vec![
        ProfileSummary {
            id: "work".into(),
            revision: "rev-work".into(),
        },
        ProfileSummary {
            id: "home".into(),
            revision: "rev-home".into(),
        },
    ];
    let home_json = default_config();
    app.ui.profile_cache.insert(
        "work".into(),
        Profile {
            id: "work".into(),
            json: app.editor.clone(),
            revision: "rev-work".into(),
        },
    );
    app.ui.profile_cache.insert(
        "home".into(),
        Profile {
            id: "home".into(),
            json: home_json.clone(),
            revision: "rev-home".into(),
        },
    );

    let work_pac = proxy_model::PacDraft {
        mode: proxy_model::PacMode::ProxyList,
        geosites: vec!["google".into()],
        domains: vec!["work-only.example".into()],
        script: String::new(),
        port: proxy_model::DEFAULT_PAC_PORT,
    };
    app.save_pac_draft(work_pac.clone());
    app.mode = ProxyMode::Pac;
    app.save_profile_proxy_mode("work", ProxyMode::Pac);

    // Switching to another profile loads that profile's independent ProxyMode & PacDraft.
    app.switch_profile("home".into());
    assert_eq!(app.profile_id, "home");
    assert_eq!(app.mode, ProxyMode::Direct);
    assert_eq!(app.pac_draft().mode, proxy_model::PacMode::AutoRules);
    assert!(app.pac_draft().domains.is_empty());

    // Editing home profile's proxy mode & PAC strategy never overwrites work.
    app.mode = ProxyMode::Global;
    app.save_profile_proxy_mode("home", ProxyMode::Global);
    let home_pac = proxy_model::PacDraft {
        mode: proxy_model::PacMode::DirectList,
        geosites: vec!["cn".into()],
        domains: vec!["home.lan".into()],
        script: String::new(),
        port: proxy_model::DEFAULT_PAC_PORT,
    };
    app.save_pac_draft(home_pac.clone());

    // Switching back to work restores work's remembered PAC mode and strategy.
    app.switch_profile("work".into());
    assert_eq!(app.profile_id, "work");
    assert_eq!(app.mode, ProxyMode::Pac);
    assert_eq!(app.pac_draft(), work_pac);

    // Persisted workspace retains both profiles' independent strategies.
    let reloaded = crate::workspace::Workspace::load(dir.path()).unwrap();
    assert_eq!(reloaded.proxy_modes.get("work"), Some(&ProxyMode::Pac));
    assert_eq!(reloaded.proxy_modes.get("home"), Some(&ProxyMode::Global));
    assert_eq!(reloaded.pac.get("work"), Some(&work_pac));
    assert_eq!(reloaded.pac.get("home"), Some(&home_pac));
}

#[test]
fn pac_page_follows_active_profile_and_auto_compiles_on_apply() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.pending.clear();

    // 1. Applying a config in ProxyMode::Pac from Profiles auto-compiles its fresh PAC script
    //    even if the user never visited the PAC page first.
    app.mode = ProxyMode::Pac;
    app.status
        .as_mut()
        .unwrap()
        .generation
        .as_mut()
        .unwrap()
        .proxy_mode = ProxyMode::Pac;
    let candidate = Candidate {
        id: "cand-sync".into(),
        digest: "digest-sync".into(),
        profile_id: "work".into(),
        base_revision: app.revision.clone(),
        revision: hash(app.editor.as_bytes()),
        config_path: "config.json".into(),
        core: CoreIdentity {
            path: "core".into(),
            sha256: "sha".into(),
            version: "Xray test".into(),
        },
        resources: Resources {
            id: "res".into(),
            root: "resources".into(),
            files: vec![],
            complete: true,
            notes: vec![],
            dynamic: vec![],
        },
    };
    app.accept_candidate_with_mode(candidate, true);
    let req = requests.try_recv().unwrap();
    let Command::ApplyProfile {
        pac: Some(applied_pac),
        ..
    } = req.command
    else {
        panic!("expected ApplyProfile with compiled PAC");
    };
    assert!(!applied_pac.script.is_empty());
    app.busy = false;
    app.pending.clear();
    let applied_rev = hash(applied_pac.script.as_bytes());
    let status = app.status.as_mut().unwrap();
    status.proxy_state = "Owned".into();
    status.pac_state = "Serving".into();
    let g = status.generation.as_mut().unwrap();
    g.proxy_mode = ProxyMode::Pac;
    g.pac = Some(PacPublication {
        revision: applied_rev.clone(),
        sha256: applied_rev,
        url: "http://127.0.0.1:18080/pac/synced/proxy.pac".into(),
        port: 18080,
        path: PathBuf::from("runtime/pac/synced/proxy.pac"),
        mode: Some(proxy_model::PacMode::AutoRules),
        summary: Some("自动生成".into()),
    });

    // 2. Browsing another unapplied profile in Profiles keeps PAC page linked with the active profile
    //    without triggering a spurious mismatch warning.
    app.profile_id = "other".into();
    app.selected_profile = "other".into();
    app.editor = r#"{"inbounds":[{"tag":"http-other","listen":"127.0.0.1","port":19999,"protocol":"http"}],"outbounds":[{"tag":"direct","protocol":"freedom"}]}"#.into();
    app.ui
        .workspace
        .names
        .insert("other".into(), "备用配置".into());

    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.page = Page::Pac;
    let out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&out).is_empty());
    let _ = text_bounds(&out, "与当前激活配置及本页策略一致，无需重新应用。");
    let _ = text_bounds(&out, "已自动跟随当前激活配置");

    // 3. Modifying PAC settings while PAC is running immediately hot-syncs via SetProxyMode(Pac).
    guided_click(
        &mut app,
        &ctx,
        "按当前路由规则自动生成（推荐）",
        1100.0,
        900.0,
    );
    let opened = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    let choice = text_bounds(&opened, "仅代理选中 GeoSite 与域名，其余直连").center();
    guided_frame(&mut app, &ctx, pointer_click(choice), 1100.0, 900.0);
    let synced_req = requests.try_recv().unwrap();
    assert!(matches!(
        synced_req.command,
        Command::SetProxyMode {
            mode: ProxyMode::Pac,
            pac: Some(_),
            ..
        }
    ));
}

#[test]
fn overview_hides_recent_activity_and_version_by_default_and_toggles_via_settings() {
    let mut app = app();
    let ctx = egui::Context::default();
    configure(&ctx);
    let has_text = |out: &egui::FullOutput, needle: &str| {
        out.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains(needle))
        })
    };

    assert!(!app.ui.workspace.overview_show_activity);
    app.ui.page = Page::Overview;
    let default_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&default_out).is_empty());
    assert!(!has_text(&default_out, "最近活动与版本"));
    assert!(!has_text(&default_out, "下次检查使用"));

    app.ui.page = Page::Settings;
    app.ui.settings_tab = SettingsTab::Appearance;
    guided_click(
        &mut app,
        &ctx,
        "在总览页显示「最近活动与版本」",
        1100.0,
        900.0,
    );
    assert!(app.ui.workspace.overview_show_activity);

    app.ui.page = Page::Overview;
    let shown_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&shown_out).is_empty());
    let _ = text_bounds(&shown_out, "最近活动与版本");
    let _ = text_bounds(&shown_out, "下次检查使用");

    guided_click(&mut app, &ctx, "隐藏", 1100.0, 900.0);
    assert!(!app.ui.workspace.overview_show_activity);
    let hidden_again = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(!has_text(&hidden_again, "最近活动与版本"));
}

#[test]
fn ssh_credentials_hosts_forwards_and_overview_on_demand_start_stop() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.pending.clear();
    app.status.as_mut().unwrap().core_state = "Stopped".into();
    app.status.as_mut().unwrap().generation = None;
    app.ui.runtime = None;
    app.ui.ssh_loaded = true;
    app.ui.ssh_state = SshState {
        credentials: vec![SshCredentialSummary {
            id: "cred-1".into(),
            name: "运维账号".into(),
            username: "ops".into(),
            auth_kind: SshAuthKind::Password,
            has_password: true,
            private_key_path: PathBuf::new(),
            has_passphrase: false,
        }],
        hosts: vec![SshHost {
            id: "host-1".into(),
            name: "跳板机 A".into(),
            host: "10.0.0.2".into(),
            port: 22,
            credential_id: "cred-1".into(),
            host_key_fingerprint: "SHA256:abcdef".into(),
        }],
        forwards: vec![SshForward {
            id: "fwd-ssh-1".into(),
            name: "测试数据库".into(),
            listen: "127.0.0.1".into(),
            port: 15432,
            target_host: "10.0.1.8".into(),
            target_port: 5432,
            ssh_host_id: "host-1".into(),
        }],
        runtime: vec![SshForwardRuntime {
            forward_id: "fwd-ssh-1".into(),
            state: SshForwardState::Stopped,
            last_error: None,
        }],
    };

    let ctx = egui::Context::default();
    configure(&ctx);

    // 1. Overview shows SSH forward with Start button even when Core is stopped.
    app.ui.page = Page::Overview;
    let ov1 = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&ov1).is_empty());
    let _ = text_bounds(&ov1, "测试数据库");
    let _ = text_bounds(&ov1, "已停止");
    guided_click(&mut app, &ctx, "启动", 1100.0, 900.0);
    assert!(matches!(
        requests.try_recv().unwrap().command,
        Command::StartSshForward { ref id } if id == "fwd-ssh-1"
    ));

    // Simulate runtime transitioning to Running and pressing Stop on Overview.
    app.busy = false;
    app.pending.clear();
    app.ui.ssh_state.runtime = vec![SshForwardRuntime {
        forward_id: "fwd-ssh-1".into(),
        state: SshForwardState::Running,
        last_error: None,
    }];
    let ov2 = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&ov2).is_empty());
    let _ = text_bounds(&ov2, "运行中");
    guided_click(&mut app, &ctx, "停止", 1100.0, 900.0);
    assert!(matches!(
        requests.try_recv().unwrap().command,
        Command::StopSshForward { ref id } if id == "fwd-ssh-1"
    ));
    app.busy = false;
    app.pending.clear();

    // 2. Profiles -> PortForward shows unified forwards, SSH hosts, and SSH credentials subtabs.
    app.ui.page = Page::Profiles;
    app.ui.profile_tab = ProfileTab::PortForward;
    app.ui.pf_subtab = PortForwardSubTab::Forwards;
    let pf_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&pf_out).is_empty());
    let _ = text_bounds(&pf_out, "SSH 隧道");

    app.ui.pf_subtab = PortForwardSubTab::SshHosts;
    let host_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&host_out).is_empty());
    let _ = text_bounds(&host_out, "跳板机 A");
    let _ = text_bounds(&host_out, "指纹已固定");

    app.ui.pf_subtab = PortForwardSubTab::SshCredentials;
    let cred_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&cred_out).is_empty());
    let _ = text_bounds(&cred_out, "运维账号");
    let _ = text_bounds(&cred_out, "账号: ops");

    // 3. Selecting SSH outbound in PortForward dialog rejects UDP and accepts TCP cleanly.
    let val: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    let mut form = crate::port_forward::PortForwardForm::from_new_with_ssh(
        &val,
        &[],
        &app.ui.ssh_state.forwards,
    );
    form.name = "Redis".into();
    form.local_port = "16379".into();
    form.target_host = "10.0.1.9".into();
    form.target_port = "6379".into();
    form.outbound_is_ssh = true;
    form.ssh_host_id = "host-1".into();
    form.network = "udp".into();
    assert!(
        crate::port_forward::check_form_error_with_ssh(
            &val,
            &form,
            &app.ui.ssh_state.hosts,
            &app.ui.ssh_state.forwards,
        )
        .is_some()
    );
    form.network = "tcp".into();
    assert!(
        crate::port_forward::check_form_error_with_ssh(
            &val,
            &form,
            &app.ui.ssh_state.hosts,
            &app.ui.ssh_state.forwards,
        )
        .is_none()
    );
    // Port conflicting with existing SSH forward is caught.
    form.local_port = "15432".into();
    assert!(
        crate::port_forward::check_form_error_with_ssh(
            &val,
            &form,
            &app.ui.ssh_state.hosts,
            &app.ui.ssh_state.forwards,
        )
        .is_some()
    );
}

#[test]
fn ssh_tunnels_work_when_xray_json_is_broken() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.pending.clear();
    app.editor = "{ invalid json".into();
    app.ui.ssh_loaded = true;
    app.ui.ssh_state = SshState {
        credentials: vec![SshCredentialSummary {
            id: "cred-1".into(),
            name: "ops · 密码".into(),
            username: "ops".into(),
            auth_kind: SshAuthKind::Password,
            has_password: true,
            private_key_path: PathBuf::new(),
            has_passphrase: false,
        }],
        hosts: vec![SshHost {
            id: "host-1".into(),
            name: "跳板机 A".into(),
            host: "10.0.0.2".into(),
            port: 22,
            credential_id: "cred-1".into(),
            host_key_fingerprint: "SHA256:abcdef".into(),
        }],
        forwards: vec![SshForward {
            id: "fwd-ssh-1".into(),
            name: "PG (10.0.1.8:5432)".into(),
            listen: "127.0.0.1".into(),
            port: 15432,
            target_host: "10.0.1.8".into(),
            target_port: 5432,
            ssh_host_id: "host-1".into(),
        }],
        runtime: vec![SshForwardRuntime {
            forward_id: "fwd-ssh-1".into(),
            state: SshForwardState::Stopped,
            last_error: None,
        }],
    };

    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.page = Page::Profiles;
    app.ui.profile_tab = ProfileTab::PortForward;
    app.ui.pf_subtab = PortForwardSubTab::Forwards;

    let out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&out).is_empty());
    let has_text = |o: &egui::FullOutput, needle: &str| {
        o.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains(needle))
        })
    };
    assert!(has_text(&out, "当前 Xray 草稿存在 JSON 语法错误"));
    let _ = text_bounds(&out, "+ Xray 转发");
    // Clicking "+ Xray 转发" must be disabled when JSON is broken.
    guided_click(&mut app, &ctx, "+ Xray 转发", 1100.0, 900.0);
    assert!(app.ui.dialog.is_none());

    // Starting an existing SSH tunnel works even with broken Xray JSON.
    guided_click(&mut app, &ctx, "启动", 1100.0, 900.0);
    assert!(matches!(
        requests.try_recv().unwrap().command,
        Command::StartSshForward { ref id } if id == "fwd-ssh-1"
    ));
    app.busy = false;
    app.pending.clear();

    // Opening "+ SSH 隧道" works and creates with start_now = true.
    guided_click(&mut app, &ctx, "+ SSH 隧道", 1100.0, 900.0);
    assert!(matches!(app.ui.dialog, Some(Dialog::PortForward)));
    app.ui.pf_form.target_host = "10.0.1.9".into();
    app.ui.pf_form.target_port = "6379".into();
    guided_click(&mut app, &ctx, "保存并启动", 1100.0, 900.0);
    assert!(matches!(
        requests.try_recv().unwrap().command,
        Command::SaveSshForward { ref forward, start_now: true } if forward.name == "Redis (10.0.1.9:6379)"
    ));
    assert!(app.ui.dialog.is_none());
}

#[test]
fn ssh_port_forward_dialog_parses_ssh_l_presets_conflict_and_autoname() {
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    let (responses, rx) = mpsc::channel();
    let mut app = App::with_channels("unused-ui-test".into(), tx, rx);
    app.ui.connected = true;
    app.ui.setup_initialized = true;
    app.ui.onboarding = false;
    app.ui.settings_loaded = true;
    app.ui.profiles_loaded = true;
    let json = default_config();
    let rev = hash(json.as_bytes());
    app.load(Profile {
        id: "work".into(),
        json,
        revision: rev,
    });
    app.pending.clear();
    while requests.try_recv().is_ok() {}
    app.ui.ssh_loaded = true;
    app.ui.ssh_state = SshState {
        credentials: vec![],
        hosts: vec![],
        forwards: vec![SshForward {
            id: "existing-ssh".into(),
            name: "Existing".into(),
            listen: "127.0.0.1".into(),
            port: 15432,
            target_host: "10.0.1.2".into(),
            target_port: 5432,
            ssh_host_id: "other".into(),
        }],
        runtime: vec![],
    };

    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.page = Page::Overview;
    guided_click(&mut app, &ctx, "+ SSH 隧道", 1100.0, 900.0);
    assert!(matches!(app.ui.dialog, Some(Dialog::PortForward)));

    // 1. Parse ssh -L command into form fields and jump to inline host creation.
    app.ui.pf_form.ssh_cmd_input = "ssh -L 15432:10.0.1.8:5432 ops@bastion.corp -p 2222".into();
    guided_click(&mut app, &ctx, "解析填入", 1100.0, 900.0);
    assert_eq!(app.ui.pf_form.local_port, "15432");
    assert_eq!(app.ui.pf_form.target_host, "10.0.1.8");
    assert_eq!(app.ui.pf_form.target_port, "5432");
    assert!(app.ui.pf_form.parsed_ssh_hint.is_some());

    guided_click(&mut app, &ctx, "按解析结果新建跳板机", 1100.0, 900.0);
    assert!(matches!(app.ui.dialog, Some(Dialog::SshHost)));
    assert_eq!(app.ui.ssh_host_form.host, "bastion.corp");
    assert_eq!(app.ui.ssh_host_form.port, "2222");
    assert_eq!(app.ui.ssh_host_form.cred_mode, SshHostCredMode::Inline);
    assert_eq!(app.ui.ssh_host_form.inline_username, "ops");
    assert!(app.ui.ssh_host_form.return_to_pf);

    // Save inline host + credential with blank host name & cred name -> auto-named.
    app.ui.ssh_host_form.inline_password = "pw-123".into();
    app.ui.ssh_host_form.host_key_fingerprint = "SHA256:testfp".into();
    guided_click(&mut app, &ctx, "保存跳板机", 1100.0, 900.0);
    let host_req = requests.try_recv().unwrap();
    let new_state = match &host_req.command {
        Command::SaveSshHost {
            host,
            inline_credential: Some(cred),
        } => {
            assert_eq!(host.name, "bastion.corp");
            assert_eq!(cred.name, "ops · 密码");
            assert_eq!(cred.username, "ops");
            SshState {
                credentials: vec![SshCredentialSummary {
                    id: "cred-new".into(),
                    name: cred.name.clone(),
                    username: cred.username.clone(),
                    auth_kind: SshAuthKind::Password,
                    has_password: true,
                    private_key_path: PathBuf::new(),
                    has_passphrase: false,
                }],
                hosts: vec![SshHost {
                    id: "host-new".into(),
                    name: host.name.clone(),
                    host: host.host.clone(),
                    port: host.port,
                    credential_id: "cred-new".into(),
                    host_key_fingerprint: host.host_key_fingerprint.clone(),
                }],
                forwards: app.ui.ssh_state.forwards.clone(),
                runtime: vec![],
            }
        }
        other => panic!("unexpected command: {other:?}"),
    };
    responses
        .send((
            host_req.command,
            Response {
                request_id: host_req.request_id,
                result: Ok(serde_json::to_value(new_state).unwrap()),
            },
        ))
        .unwrap();
    app.receive();
    assert!(matches!(app.ui.dialog, Some(Dialog::PortForward)));
    assert_eq!(app.ui.pf_form.ssh_host_id, "host-new");

    // 2. Port 15432 conflicts with existing-ssh -> fix via "改用空闲端口 (15433)", switch preset to 3306 MySQL, leave name blank.
    guided_click(&mut app, &ctx, "改用空闲端口 (15433)", 1100.0, 900.0);
    assert_eq!(app.ui.pf_form.local_port, "15433");
    guided_click(&mut app, &ctx, "3306 MySQL", 1100.0, 900.0);
    assert_eq!(app.ui.pf_form.target_port, "3306");
    guided_click(&mut app, &ctx, "保存并启动", 1100.0, 900.0);
    assert!(matches!(
        requests.try_recv().unwrap().command,
        Command::SaveSshForward { ref forward, start_now: true }
            if forward.name == "MySQL (10.0.1.8:3306)" && forward.port == 15433 && forward.ssh_host_id == "host-new"
    ));
}

#[test]
fn ssh_host_inline_paste_quick_forward_and_test_connection() {
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    let (responses, rx) = mpsc::channel();
    let mut app = App::with_channels("unused-ui-test".into(), tx, rx);
    app.ui.connected = true;
    app.ui.setup_initialized = true;
    app.ui.onboarding = false;
    app.ui.settings_loaded = true;
    app.ui.profiles_loaded = true;
    let json = default_config();
    let rev = hash(json.as_bytes());
    app.load(Profile {
        id: "work".into(),
        json,
        revision: rev,
    });
    app.pending.clear();
    while requests.try_recv().is_ok() {}
    app.ui.ssh_loaded = true;

    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.page = Page::Profiles;
    app.ui.profile_tab = ProfileTab::PortForward;
    app.ui.pf_subtab = PortForwardSubTab::SshHosts;

    // With 0 credentials, opening "+ 新增 SSH 跳板机" defaults to Inline credential mode.
    guided_click(&mut app, &ctx, "+ 新增 SSH 跳板机", 1100.0, 900.0);
    assert!(matches!(app.ui.dialog, Some(Dialog::SshHost)));
    assert_eq!(app.ui.ssh_host_form.cred_mode, SshHostCredMode::Inline);

    // Pasting user@host:port into server address and blurring splits host, port, and inline username.
    let modal_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    let addr_pos = text_bounds(&modal_out, "bastion.example.com 或 ops@bastion.corp:2222").center();
    guided_frame(&mut app, &ctx, pointer_click(addr_pos), 1100.0, 900.0);
    guided_frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("alice@jump.internal:2202".into())],
        1100.0,
        900.0,
    );
    let port_pos = text_bounds(&modal_out, "SSH 端口").center();
    guided_frame(&mut app, &ctx, pointer_click(port_pos), 1100.0, 900.0);
    assert_eq!(app.ui.ssh_host_form.host, "jump.internal");
    assert_eq!(app.ui.ssh_host_form.port, "2202");
    assert_eq!(app.ui.ssh_host_form.inline_username, "alice");
    app.ui.dialog = None;

    // Populate a host and exercise "+ 转发" and "测试连接".
    app.ui.ssh_state = SshState {
        credentials: vec![SshCredentialSummary {
            id: "cred-1".into(),
            name: "alice · 密码".into(),
            username: "alice".into(),
            auth_kind: SshAuthKind::Password,
            has_password: true,
            private_key_path: PathBuf::new(),
            has_passphrase: false,
        }],
        hosts: vec![SshHost {
            id: "host-1".into(),
            name: "跳板机 1".into(),
            host: "jump.internal".into(),
            port: 2202,
            credential_id: "cred-1".into(),
            host_key_fingerprint: "SHA256:fp1".into(),
        }],
        forwards: vec![],
        runtime: vec![],
    };

    guided_click(&mut app, &ctx, "+ 转发", 1100.0, 900.0);
    assert!(matches!(app.ui.dialog, Some(Dialog::PortForward)));
    assert!(app.ui.pf_form.outbound_is_ssh);
    assert_eq!(app.ui.pf_form.ssh_host_id, "host-1");
    app.ui.dialog = None;

    guided_click(&mut app, &ctx, "测试连接", 1100.0, 900.0);
    let test_req = requests.try_recv().unwrap();
    assert!(matches!(
        test_req.command,
        Command::TestSshHost { ref host_id, .. } if host_id == "host-1"
    ));
    assert!(!app.busy);
    responses
        .send((
            test_req.command,
            Response {
                request_id: test_req.request_id,
                result: Ok(serde_json::to_value(SshHostTestResult {
                    host: "jump.internal".into(),
                    port: 2202,
                    username: "alice".into(),
                    latency_ms: 19,
                })
                .unwrap()),
            },
        ))
        .unwrap();
    app.receive();
    let tested_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&tested_out).is_empty());
    assert!(tested_out.shapes.iter().any(|s| {
        matches!(&s.shape, egui::Shape::Text(t) if t.galley.text().contains("握手与身份校验通过"))
    }));
}

#[test]
fn overview_and_forwards_batch_controls_copy_clone_and_theme_layout() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    app.pending.clear();
    app.ui.ssh_loaded = true;
    app.ui.ssh_state = SshState {
        credentials: vec![SshCredentialSummary {
            id: "cred-1".into(),
            name: "ops · 密码".into(),
            username: "ops".into(),
            auth_kind: SshAuthKind::Password,
            has_password: true,
            private_key_path: PathBuf::new(),
            has_passphrase: false,
        }],
        hosts: vec![SshHost {
            id: "host-1".into(),
            name: "生产跳板机".into(),
            host: "10.0.0.2".into(),
            port: DEFAULT_SSH_PORT,
            credential_id: "cred-1".into(),
            host_key_fingerprint: "SHA256:fp".into(),
        }],
        forwards: vec![
            SshForward {
                id: "sf-1".into(),
                name: "PG (10.0.1.8:5432)".into(),
                listen: "127.0.0.1".into(),
                port: 15432,
                target_host: "10.0.1.8".into(),
                target_port: 5432,
                ssh_host_id: "host-1".into(),
            },
            SshForward {
                id: "sf-2".into(),
                name: "MySQL (10.0.1.9:3306)".into(),
                listen: "0.0.0.0".into(),
                port: 13306,
                target_host: "10.0.1.9".into(),
                target_port: 3306,
                ssh_host_id: "host-1".into(),
            },
        ],
        runtime: vec![
            SshForwardRuntime {
                forward_id: "sf-1".into(),
                state: SshForwardState::Running,
                last_error: None,
            },
            SshForwardRuntime {
                forward_id: "sf-2".into(),
                state: SshForwardState::Stopped,
                last_error: None,
            },
        ],
    };

    let ctx = egui::Context::default();
    configure(&ctx);

    for appearance in [Appearance::Light, Appearance::Dark] {
        appearance.apply(&ctx);
        for (w, h) in [(1100.0, 900.0), (760.0, 500.0)] {
            app.ui.page = Page::Overview;
            let ov = guided_frame(&mut app, &ctx, vec![], w, h);
            assert!(overlapping_visible_text(&ov).is_empty());
            if w >= 1000.0 {
                let _ = text_bounds(&ov, "全部启动");
                let _ = text_bounds(&ov, "全部停止");
                let _ = text_bounds(&ov, "复制");
            }

            app.ui.page = Page::Profiles;
            app.ui.profile_tab = ProfileTab::PortForward;
            app.ui.pf_subtab = PortForwardSubTab::Forwards;
            let pf = guided_frame(&mut app, &ctx, vec![], w, h);
            assert!(overlapping_visible_text(&pf).is_empty());
            if w >= 1000.0 {
                let _ = text_bounds(&pf, "全部启动");
                let _ = text_bounds(&pf, "全部停止");
                let _ = text_bounds(&pf, "复制地址");
                let _ = text_bounds(&pf, "克隆");
            }
        }
    }

    // Batch actions send StartAllSshForwards / StopAllSshForwards, and Clone opens free port.
    app.ui.page = Page::Overview;
    guided_click(&mut app, &ctx, "全部启动", 1100.0, 760.0);
    assert!(matches!(
        requests.try_recv().unwrap().command,
        Command::StartAllSshForwards
    ));
    app.busy = false;
    app.pending.clear();

    guided_click(&mut app, &ctx, "全部停止", 1100.0, 760.0);
    assert!(matches!(
        requests.try_recv().unwrap().command,
        Command::StopAllSshForwards
    ));
    app.busy = false;
    app.pending.clear();

    app.ui.page = Page::Profiles;
    app.ui.profile_tab = ProfileTab::PortForward;
    app.ui.pf_subtab = PortForwardSubTab::Forwards;
    guided_click(&mut app, &ctx, "克隆", 1100.0, 900.0);
    assert!(matches!(app.ui.dialog, Some(Dialog::PortForward)));
    assert!(app.ui.pf_form.is_clone);
    assert!(app.ui.pf_form.start_now);
    assert_eq!(app.ui.pf_form.local_port, "15433");
}

#[test]
fn modal_dialogs_use_balanced_widths_padding_and_harmonized_title_size() {
    let measure_modal = |appearance: Appearance, dialog: Dialog, title: &str, size: (f32, f32)| {
        let mut app = app();
        app.ui.history_preview = Some(HistoryPreview {
            json: app.editor.clone(),
            core: "xray.exe".into(),
        });
        app.ui.dialog = Some(dialog);
        let ctx = egui::Context::default();
        configure(&ctx);
        appearance.apply(&ctx);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0, size.1));
        let mut output = None;
        for frame in 0..8 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    time: Some(frame as f64 * 0.05),
                    ..Default::default()
                },
                |ctx| app.render(ctx),
            ));
        }
        let output = output.unwrap();
        let title_rect = text_bounds(&output, title);
        let mut rects = vec![];
        for shape in &output.shapes {
            painted_rects(&shape.shape, &mut rects);
        }
        let modal = rects
            .into_iter()
            .rev()
            .find(|rect| {
                rect.contains(title_rect.center()) && rect.height() >= CONTROL_HEIGHT * 2.0
            })
            .unwrap();
        assert!(viewport.contains_rect(modal), "{title}: overflow {modal:?}");
        // 18px title + 20x16 padding keeps title comfortably inset from modal border.
        assert!(title_rect.height() < 28.0, "title too tall: {title_rect:?}");
        assert!(title_rect.left() - modal.left() >= 18.0);
        assert!(title_rect.top() - modal.top() >= 14.0);
        modal
    };

    for appearance in [Appearance::Light, Appearance::Dark] {
        let confirm = measure_modal(
            appearance,
            Dialog::Confirm(Action::Stop),
            "停止 Core？",
            (1100.0, 760.0),
        );
        let exit_choice = measure_modal(
            appearance,
            Dialog::ExitChoice,
            "退出 Proxy Pilot",
            (1100.0, 760.0),
        );
        let port_forward = measure_modal(
            appearance,
            Dialog::PortForward,
            "添加端口转发",
            (1100.0, 760.0),
        );
        let ssh_cred = measure_modal(
            appearance,
            Dialog::SshCredential,
            "新建 SSH 凭证",
            (1100.0, 760.0),
        );
        let ssh_host = measure_modal(
            appearance,
            Dialog::SshHost,
            "新建 SSH 跳板机",
            (1100.0, 760.0),
        );
        let create = measure_modal(appearance, Dialog::Create, "新建配置", (1100.0, 760.0));
        let pac_batch = measure_modal(
            appearance,
            Dialog::PacBatch,
            "批量导入 / 编辑 PAC 域名",
            (1100.0, 760.0),
        );
        assert!(confirm.width() < port_forward.width());
        assert!(exit_choice.width() < port_forward.width());
        assert!(port_forward.width() < create.width());
        assert!((ssh_cred.width() - ssh_host.width()).abs() < 1.0);
        assert!((create.width() - pac_batch.width()).abs() < 1.0);
        assert!(create.width() < 840.0);

        for (dialog, title) in [
            (Dialog::ExitChoice, "退出 Proxy Pilot"),
            (Dialog::PortForward, "添加端口转发"),
            (Dialog::SshCredential, "新建 SSH 凭证"),
            (Dialog::SshHost, "新建 SSH 跳板机"),
            (Dialog::HistoryDiff, "比较历史版本与当前草稿"),
            (Dialog::PacBatch, "批量导入 / 编辑 PAC 域名"),
        ] {
            let small = measure_modal(appearance, dialog, title, (760.0, 500.0));
            assert!(small.width() < 740.0, "{title}: {small:?}");
        }
    }
}

#[test]
fn top_bar_places_version_label_in_top_right_corner() {
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let actual_bounds = |output: &egui::FullOutput, label: &str| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing rendered label: {label}"))
    };
    for appearance in [Appearance::Light, Appearance::Dark] {
        for width in [760.0, 1020.0, 1100.0] {
            let mut app = app();
            let ctx = egui::Context::default();
            configure(&ctx);
            appearance.apply(&ctx);
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 760.0));
            let mut output = None;
            for frame in 0..4 {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(viewport),
                        time: Some(frame as f64 * 0.05),
                        ..Default::default()
                    },
                    |ctx| app.render(ctx),
                ));
            }
            let output = output.unwrap();
            let brand_rect = actual_bounds(&output, "Proxy Pilot");
            let version_rect = actual_bounds(&output, &version);
            let settings_rect = actual_bounds(&output, "设置");
            assert!(
                viewport.contains_rect(version_rect),
                "width={width}: {version_rect:?}"
            );
            assert!(
                version_rect.right() > width - 48.0,
                "expected top-right corner at width={width}: {version_rect:?}"
            );
            assert!(
                (version_rect.center().y - brand_rect.center().y).abs() < 6.0,
                "expected version on top row at width={width}: brand={brand_rect:?}, version={version_rect:?}"
            );
            if width >= 1020.0 {
                let nav_rects = ["总览", "配置", "PAC", "日志", "诊断", "设置"]
                    .map(|label| actual_bounds(&output, label));
                for pair in nav_rects.windows(2) {
                    assert!(
                        pair[0].right() < pair[1].left(),
                        "nav order broken at width={width}: {pair:?}"
                    );
                }
                assert!(
                    settings_rect.right() < version_rect.left(),
                    "expected version right of nav at width={width}: settings={settings_rect:?}, version={version_rect:?}"
                );
            }
        }
    }
}

#[test]
fn startup_conflict_failure_offers_confirmation_and_sends_resolve_command() {
    let (tx, requests) = mpsc::sync_channel(8);
    let (_responses, rx) = mpsc::channel();
    let mut base = app();
    let status = base.status.take().unwrap();
    let mut app = App::with_channels("unused-ui-test".into(), tx, rx);
    app.ui.connected = true;
    app.ui.setup_initialized = true;
    app.ui.onboarding = false;
    app.status = Some(status);
    app.ui.failure = Some(crate::app::ui::Failure {
        error: Error::new("STARTUP_CONFLICT", "Login task ownership/action changed"),
        request: "req-startup".into(),
    });
    assert!(app.can_resolve_startup_conflict());

    let ctx = egui::Context::default();
    configure(&ctx);
    let frame = |app: &mut App, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };

    let output = frame(&mut app, vec![]);
    let resolve_btn = text_bounds(&output, "重新接管自启计划任务…").center();
    frame(&mut app, pointer_click(resolve_btn));
    for _ in 0..3 {
        let _ = frame(&mut app, vec![]);
    }
    let modal = frame(&mut app, vec![]);
    assert!(matches!(
        app.ui.dialog,
        Some(Dialog::Confirm(Action::ResolveStartupConflict))
    ));

    let confirm_btn = text_bounds(&modal, "确认重新接管").center();
    frame(&mut app, pointer_click(confirm_btn));
    assert!(
        requests
            .try_iter()
            .any(|r| matches!(r.command, Command::ResolveStartupConflict))
    );
}

#[test]
fn overview_action_buttons_match_standard_control_height() {
    let mut app = app();
    app.ui.page = Page::Overview;
    app.ui.workspace.overview_show_activity = true;
    let ctx = egui::Context::default();
    configure(&ctx);

    // 1. Running Core state: Core & profile controls live inside the left card alongside mode pills.
    let output = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&output).is_empty());

    let running_expected = [
        "停止 Core",
        "重启 Core",
        "管理配置",
        "路由诊断",
        "管理转发",
        "直连",
        "PAC",
        "全局",
        "查看日志",
        "隐藏",
    ];
    for label in running_expected {
        let rect = enclosing_rect(&output, label, CONTROL_HEIGHT);
        assert!(
            (rect.height() - CONTROL_HEIGHT).abs() < 1.0,
            "overview button {label} height mismatch: {rect:?}"
        );
    }
    let stop_rect = enclosing_rect(&output, "停止 Core", CONTROL_HEIGHT);
    let restart_rect = enclosing_rect(&output, "重启 Core", CONTROL_HEIGHT);
    let diag_rect = enclosing_rect(&output, "路由诊断", CONTROL_HEIGHT);
    let mode_rect = enclosing_rect(&output, "全局", CONTROL_HEIGHT);
    assert!(stop_rect.top() > mode_rect.bottom());
    assert!((stop_rect.center().y - restart_rect.center().y).abs() < 1.0);
    assert!((stop_rect.center().y - diag_rect.center().y).abs() < 1.0);

    // 2. Stopped with saved generation: "启动 Core" + "管理配置" + "路由诊断".
    app.status.as_mut().unwrap().core_state = "Stopped".into();
    let stopped_gen_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&stopped_gen_out).is_empty());
    for label in ["启动 Core", "管理配置", "路由诊断"] {
        let rect = enclosing_rect(&stopped_gen_out, label, CONTROL_HEIGHT);
        assert!((rect.height() - CONTROL_HEIGHT).abs() < 1.0);
    }

    // 3. Stopped without any generation: "选择配置" + "路由诊断".
    app.status.as_mut().unwrap().generation = None;
    let fresh_out = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    assert!(overlapping_visible_text(&fresh_out).is_empty());
    for label in ["选择配置", "路由诊断"] {
        let rect = enclosing_rect(&fresh_out, label, CONTROL_HEIGHT);
        assert!((rect.height() - CONTROL_HEIGHT).abs() < 1.0);
    }
}

#[test]
fn overview_ssh_tunnel_cards_and_badges_never_clip_or_collapse_vertically() {
    let mut app = app();
    app.ui.ssh_loaded = true;
    app.ui
        .workspace
        .names
        .insert("work".into(), "75-x600".into());
    app.status.as_mut().unwrap().proxy_state = "Owned".into();
    if let Some(ref mut g) = app.status.as_mut().unwrap().generation {
        g.proxy_mode = ProxyMode::Pac;
    }
    app.ui.ssh_state = SshState {
        credentials: vec![SshCredentialSummary {
            id: "cred-1".into(),
            name: "ops · 密码".into(),
            username: "ops".into(),
            auth_kind: SshAuthKind::Password,
            has_password: true,
            private_key_path: PathBuf::new(),
            has_passphrase: false,
        }],
        hosts: vec![
            SshHost {
                id: "host-vpc".into(),
                name: "workstation.vpc".into(),
                host: "10.1.0.2".into(),
                port: DEFAULT_SSH_PORT,
                credential_id: "cred-1".into(),
                host_key_fingerprint: "SHA256:vpc".into(),
            },
            SshHost {
                id: "host-edge".into(),
                name: "edge.mulife.vip".into(),
                host: "198.51.100.10".into(),
                port: DEFAULT_SSH_PORT,
                credential_id: "cred-1".into(),
                host_key_fingerprint: "SHA256:edge".into(),
            },
        ],
        forwards: vec![
            SshForward {
                id: "sf-1".into(),
                name: "Web (127.0.0.1:8080)".into(),
                listen: "127.0.0.1".into(),
                port: 18010,
                target_host: "127.0.0.1".into(),
                target_port: 8080,
                ssh_host_id: "host-vpc".into(),
            },
            SshForward {
                id: "sf-2".into(),
                name: "Web (127.0.0.1:8080)".into(),
                listen: "127.0.0.1".into(),
                port: 18020,
                target_host: "127.0.0.1".into(),
                target_port: 8080,
                ssh_host_id: "host-edge".into(),
            },
        ],
        runtime: vec![
            SshForwardRuntime {
                forward_id: "sf-1".into(),
                state: SshForwardState::Stopped,
                last_error: None,
            },
            SshForwardRuntime {
                forward_id: "sf-2".into(),
                state: SshForwardState::Running,
                last_error: None,
            },
        ],
    };

    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.page = Page::Overview;

    for appearance in [Appearance::Light, Appearance::Dark] {
        appearance.apply(&ctx);
        // At 760x500 the stacked second card can scroll below y=500; verify visible widgets
        // don't overlap or crush, then verify full in-viewport placement across larger sizes.
        let mut small_ov = guided_frame(&mut app, &ctx, vec![], 760.0, 500.0);
        for _ in 0..3 {
            small_ov = guided_frame(&mut app, &ctx, vec![], 760.0, 500.0);
        }
        assert!(overlapping_visible_text(&small_ov).is_empty());
        for shape in &small_ov.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && matches!(text.galley.text(), "运行中" | "已停止")
            {
                assert_eq!(text.galley.rows.len(), 1);
            }
        }
        for (w, h) in [
            (760.0, 1200.0),
            (980.0, 760.0),
            (1100.0, 760.0),
            (1440.0, 900.0),
        ] {
            let mut ov = guided_frame(&mut app, &ctx, vec![], w, h);
            for _ in 0..3 {
                ov = guided_frame(&mut app, &ctx, vec![], w, h);
            }
            assert!(
                overlapping_visible_text(&ov).is_empty(),
                "overlap at {appearance:?} {w}x{h}"
            );
            for shape in &ov.shapes {
                if let egui::Shape::Text(text) = &shape.shape
                    && matches!(text.galley.text(), "运行中" | "已停止")
                {
                    assert_eq!(
                        text.galley.rows.len(),
                        1,
                        "badge collapsed at {appearance:?} {w}x{h}: rows={}",
                        text.galley.rows.len()
                    );
                }
            }
            for label in [
                "全部启动",
                "全部停止",
                "复制",
                "启动",
                "停止",
                "+ SSH 隧道",
                "管理转发",
            ] {
                let mut found = false;
                for shape in &ov.shapes {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.text() == label
                    {
                        found = true;
                        let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                        assert!(
                            rect.left() >= 0.0 && rect.right() <= w - 16.0,
                            "{label} clipped at {appearance:?} {w}x{h}: {rect:?}"
                        );
                        assert!(
                            shape.clip_rect.contains_rect(rect),
                            "{label} clipped by parent at {appearance:?} {w}x{h}: rect={rect:?}, clip={:?}",
                            shape.clip_rect
                        );
                    }
                }
                assert!(found, "missing {label} at {appearance:?} {w}x{h}");
            }
        }
    }
}

#[test]
fn profiles_page_adapts_across_window_sizes_without_clipping_headers_or_controls() {
    let mut app = app();
    app.ui.ssh_loaded = true;

    // Populate a realistic profile with an extra outbound, routing rules, and Xray forward.
    let mut value: serde_json::Value = serde_json::from_str(&app.editor).unwrap();
    value["outbounds"].as_array_mut().unwrap().insert(
        0,
        serde_json::json!({
            "tag": "node-1",
            "protocol": "vless",
            "settings": {
                "vnext": [{
                    "address": "hk-edge.example.com",
                    "port": 443,
                    "users": [{"id": "11111111-1111-1111-1111-111111111111", "encryption": "none"}]
                }]
            },
            "streamSettings": {"network": "tcp", "security": "tls"}
        }),
    );
    app.ui
        .workspace
        .node_names
        .entry(app.profile_id.clone())
        .or_default()
        .insert("node-1".into(), "香港节点 01".into());

    let mut form = crate::port_forward::PortForwardForm::from_new(&value, &[]);
    form.name = "内网管理端".into();
    form.local_port = "18443".into();
    form.target_host = "10.0.0.8".into();
    form.target_port = "443".into();
    form.outbound_tag = "direct".into();
    crate::port_forward::save_entry(&mut value, &mut app.ui.workspace, &app.profile_id, &form)
        .unwrap();

    value["routing"]["rules"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "type": "field",
            "domain": ["geosite:cn"],
            "outboundTag": "direct"
        }));
    app.editor = serde_json::to_string_pretty(&value).unwrap();

    app.ui.ssh_state = SshState {
        credentials: vec![SshCredentialSummary {
            id: "cred-1".into(),
            name: "ops · 密码".into(),
            username: "ops".into(),
            auth_kind: SshAuthKind::Password,
            has_password: true,
            private_key_path: PathBuf::new(),
            has_passphrase: false,
        }],
        hosts: vec![SshHost {
            id: "host-1".into(),
            name: "prod-bastion.internal".into(),
            host: "198.51.100.22".into(),
            port: DEFAULT_SSH_PORT,
            credential_id: "cred-1".into(),
            host_key_fingerprint: "SHA256:fp-prod".into(),
        }],
        forwards: vec![
            SshForward {
                id: "sf-1".into(),
                name: "PG (10.0.1.8:5432)".into(),
                listen: "127.0.0.1".into(),
                port: 15432,
                target_host: "10.0.1.8".into(),
                target_port: 5432,
                ssh_host_id: "host-1".into(),
            },
            SshForward {
                id: "sf-2".into(),
                name: "Redis (10.0.1.9:6379)".into(),
                listen: "0.0.0.0".into(),
                port: 16379,
                target_host: "10.0.1.9".into(),
                target_port: 6379,
                ssh_host_id: "host-1".into(),
            },
        ],
        runtime: vec![
            SshForwardRuntime {
                forward_id: "sf-1".into(),
                state: SshForwardState::Running,
                last_error: None,
            },
            SshForwardRuntime {
                forward_id: "sf-2".into(),
                state: SshForwardState::Stopped,
                last_error: None,
            },
        ],
    };

    let ctx = egui::Context::default();
    configure(&ctx);
    app.ui.page = Page::Profiles;

    let views: &[(ProfileTab, PortForwardSubTab, AdvancedTab, &[&str])] = &[
        (
            ProfileTab::Outbounds,
            PortForwardSubTab::Forwards,
            AdvancedTab::Inbounds,
            &["节点", "分流规则", "更多与源码", "直连", "PAC", "全局"],
        ),
        (
            ProfileTab::Routing,
            PortForwardSubTab::Forwards,
            AdvancedTab::Inbounds,
            &["节点", "分流规则", "更多与源码", "直连", "PAC", "全局"],
        ),
        (
            ProfileTab::PortForward,
            PortForwardSubTab::Forwards,
            AdvancedTab::Inbounds,
            &["+ SSH 隧道", "+ Xray 转发", "直连", "PAC", "全局"],
        ),
        (
            ProfileTab::PortForward,
            PortForwardSubTab::SshHosts,
            AdvancedTab::Inbounds,
            &["+ 新增 SSH 跳板机", "+ 转发", "测试连接"],
        ),
        (
            ProfileTab::PortForward,
            PortForwardSubTab::SshCredentials,
            AdvancedTab::Inbounds,
            &["+ 新增 SSH 凭证", "编辑", "删除"],
        ),
        (
            ProfileTab::Advanced,
            PortForwardSubTab::Forwards,
            AdvancedTab::Inbounds,
            &["节点", "分流规则", "更多与源码"],
        ),
        (
            ProfileTab::Advanced,
            PortForwardSubTab::Forwards,
            AdvancedTab::Json,
            &["格式化 JSON"],
        ),
        (
            ProfileTab::Advanced,
            PortForwardSubTab::Forwards,
            AdvancedTab::History,
            &["暂无已提交的历史版本。"],
        ),
    ];

    for appearance in [Appearance::Light, Appearance::Dark] {
        appearance.apply(&ctx);
        for (w, h) in [
            (760.0, 500.0),
            (760.0, 1000.0),
            (900.0, 650.0),
            (980.0, 760.0),
            (1100.0, 760.0),
            (1440.0, 900.0),
            (1920.0, 1040.0),
        ] {
            for &(tab, pf_sub, adv_sub, required_labels) in views {
                app.ui.profile_tab = tab;
                app.ui.pf_subtab = pf_sub;
                app.ui.advanced_tab = adv_sub;
                let mut out = guided_frame(&mut app, &ctx, vec![], w, h);
                for _ in 0..4 {
                    out = guided_frame(&mut app, &ctx, vec![], w, h);
                }
                assert!(
                    overlapping_visible_text(&out).is_empty(),
                    "overlap in {tab:?}/{pf_sub:?}/{adv_sub:?} at {appearance:?} {w}x{h}"
                );
                let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h));
                for shape in &out.shapes {
                    if let egui::Shape::Text(text) = &shape.shape
                        && matches!(
                            text.galley.text(),
                            "运行中"
                                | "已停止"
                                | "SSH 隧道"
                                | "TCP"
                                | "独立于 Core"
                                | "指纹已固定"
                                | "密码认证（已加密保存）"
                                | "常规分流规则"
                                | "默认访问"
                                | "已保存"
                        )
                    {
                        assert_eq!(
                            text.galley.rows.len(),
                            1,
                            "badge crushed at {tab:?}/{pf_sub:?} {appearance:?} {w}x{h}: {:?}",
                            text.galley.text()
                        );
                    }
                }
                // At 760x500 with vertical scroll inside main-scroll, lower rows scroll below the
                // fold; verify the top tab bar stays above the fold & completely unclipped here.
                let labels_to_check: &[&str] = if h <= 500.0 {
                    &["节点", "分流规则", "更多与源码"]
                } else {
                    required_labels
                };
                for label in labels_to_check {
                    let rect = text_bounds(&out, label);
                    let clip = out
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Text(t) if t.galley.text() == *label => Some(s.clip_rect),
                            _ => None,
                        })
                        .unwrap();
                    assert!(
                        viewport.contains_rect(rect),
                        "{label} outside viewport in {tab:?}/{pf_sub:?} at {appearance:?} {w}x{h}: {rect:?}"
                    );
                    assert!(
                        clip.contains_rect(rect),
                        "{label} clipped in {tab:?}/{pf_sub:?} at {appearance:?} {w}x{h}: rect={rect:?}, clip={clip:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn all_pages_render_cleanly_without_overflow_or_overlap_across_window_sizes() {
    let mut app = app();
    app.ui.ssh_loaded = true;
    app.logs = vec![
        "[Info] xray core started successfully on 0.0.0.0:10809\n[Warning] slow upstream handshake on 198.51.100.22:443".into(),
    ];
    app.add_event("信息", "已启动 PAC 与 SSH 隧道服务", "req-audit-1");
    app.ui.operation_details =
        "request_id=req-audit-1 action=ApplyProfile status=Committed\n".repeat(4);
    app.route_result = serde_json::to_string(&RouteResult {
        kind: "StaticPrediction".into(),
        match_state: "Matched".into(),
        rule_index: Some(0),
        expected_outbound: Some("direct".into()),
        actual_outbound: "Unverified".into(),
        reason: "命中第一条 domain 规则（geosite:cn）".into(),
    })
    .unwrap();
    app.ui.route_tested = Some((
        "generation-1".into(),
        RouteContext {
            host: "example.cn".into(),
            port: Some(443),
            inbound_tag: Some("http-in".into()),
            network: Some("tcp".into()),
            target_ip: None,
            source_ip: None,
            source_port: None,
            user: None,
            protocol: None,
            attrs: Default::default(),
        },
    ));
    app.ui.latency_host = "example.com".into();
    app.ui.latency_result =
        serde_json::to_string(&serde_json::json!({"address": "93.184.216.34:443", "ms": 42}))
            .unwrap();
    if let Some(status) = app.status.as_mut() {
        status.proxy_state = "Owned".into();
        status.pac_state = "Serving".into();
        status.winhttp = Some(WinHttpConfig {
            access_type: "DEFAULT_PROXY".into(),
            proxy: String::new(),
            bypass: "<local>".into(),
        });
        if let Some(g) = status.generation.as_mut() {
            g.proxy_mode = ProxyMode::Pac;
            g.pac = Some(PacPublication {
                revision: "pac-rev-123456".into(),
                sha256: "deadbeef".into(),
                url: "http://127.0.0.1:18080/pac/pac-rev-123456.js".into(),
                path: PathBuf::from("pac/pac-rev-123456.js"),
                port: 18080,
                mode: Some(proxy_model::PacMode::ProxyList),
                summary: Some("2 个 GeoSite · 3 条域名".into()),
            });
        }
    }
    let mut pac_draft = app.pac_draft();
    pac_draft.mode = proxy_model::PacMode::ProxyList;
    pac_draft.geosites = vec!["geosite:cn".into(), "geosite:private".into()];
    pac_draft.domains = vec![
        "example.com".into(),
        "*.internal.example".into(),
        "full:exact.example.org".into(),
    ];
    let pid = app.profile_id.clone();
    app.save_pac_draft_for(&pid, pac_draft);

    if let Some(status) = app.status.as_mut() {
        status.proxy_availability = Some(sample_proxy_availability_fixture());
    }

    app.ui.ssh_state = SshState {
        credentials: vec![SshCredentialSummary {
            id: "cred-1".into(),
            name: "ops · 密码".into(),
            username: "ops".into(),
            auth_kind: SshAuthKind::Password,
            has_password: true,
            private_key_path: PathBuf::new(),
            has_passphrase: false,
        }],
        hosts: vec![SshHost {
            id: "host-1".into(),
            name: "edge.mulife.vip".into(),
            host: "198.51.100.10".into(),
            port: DEFAULT_SSH_PORT,
            credential_id: "cred-1".into(),
            host_key_fingerprint: "SHA256:edge".into(),
        }],
        forwards: vec![SshForward {
            id: "sf-1".into(),
            name: "Web (127.0.0.1:8080)".into(),
            listen: "127.0.0.1".into(),
            port: 18020,
            target_host: "127.0.0.1".into(),
            target_port: 8080,
            ssh_host_id: "host-1".into(),
        }],
        runtime: vec![SshForwardRuntime {
            forward_id: "sf-1".into(),
            state: SshForwardState::Running,
            last_error: None,
        }],
    };

    let ctx = egui::Context::default();
    configure(&ctx);

    for appearance in [Appearance::Light, Appearance::Dark] {
        appearance.apply(&ctx);
        for (w, h) in [
            (760.0, 500.0),
            (900.0, 650.0),
            (980.0, 760.0),
            (1100.0, 760.0),
            (1440.0, 900.0),
            (1920.0, 1040.0),
        ] {
            struct ScreenCase {
                name: &'static str,
                onboarding: bool,
                setup_step: usize,
                page: Page,
                profile_tab: ProfileTab,
                pf_subtab: PortForwardSubTab,
                advanced_tab: AdvancedTab,
                settings_tab: SettingsTab,
            }
            let case = |name: &'static str,
                        onboarding: bool,
                        setup_step: usize,
                        page: Page,
                        profile_tab: ProfileTab,
                        pf_subtab: PortForwardSubTab,
                        advanced_tab: AdvancedTab,
                        settings_tab: SettingsTab| ScreenCase {
                name,
                onboarding,
                setup_step,
                page,
                profile_tab,
                pf_subtab,
                advanced_tab,
                settings_tab,
            };
            let mut screens = vec![
                case(
                    "setup-0",
                    true,
                    0,
                    Page::Overview,
                    ProfileTab::Outbounds,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
                case(
                    "setup-1",
                    true,
                    1,
                    Page::Overview,
                    ProfileTab::Outbounds,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
                case(
                    "setup-2",
                    true,
                    2,
                    Page::Overview,
                    ProfileTab::Outbounds,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
                case(
                    "overview",
                    false,
                    0,
                    Page::Overview,
                    ProfileTab::Outbounds,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
                case(
                    "pac",
                    false,
                    0,
                    Page::Pac,
                    ProfileTab::Outbounds,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
                case(
                    "diagnostics",
                    false,
                    0,
                    Page::Diagnostics,
                    ProfileTab::Outbounds,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
                case(
                    "logs",
                    false,
                    0,
                    Page::Logs,
                    ProfileTab::Outbounds,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                    SettingsTab::Core,
                ),
            ];
            for (name, p_tab, pf_sub, adv_sub) in [
                (
                    "profiles-outbounds",
                    ProfileTab::Outbounds,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                ),
                (
                    "profiles-routing",
                    ProfileTab::Routing,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                ),
                (
                    "profiles-pf-forwards",
                    ProfileTab::PortForward,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                ),
                (
                    "profiles-pf-hosts",
                    ProfileTab::PortForward,
                    PortForwardSubTab::SshHosts,
                    AdvancedTab::Inbounds,
                ),
                (
                    "profiles-pf-creds",
                    ProfileTab::PortForward,
                    PortForwardSubTab::SshCredentials,
                    AdvancedTab::Inbounds,
                ),
                (
                    "profiles-adv-inbounds",
                    ProfileTab::Advanced,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                ),
                (
                    "profiles-adv-json",
                    ProfileTab::Advanced,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Json,
                ),
                (
                    "profiles-adv-history",
                    ProfileTab::Advanced,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::History,
                ),
            ] {
                screens.push(case(
                    name,
                    false,
                    0,
                    Page::Profiles,
                    p_tab,
                    pf_sub,
                    adv_sub,
                    SettingsTab::Core,
                ));
            }
            for (name, s_tab) in [
                ("settings-core", SettingsTab::Core),
                ("settings-startup", SettingsTab::Startup),
                ("settings-probe", SettingsTab::Probe),
                ("settings-appearance", SettingsTab::Appearance),
                ("settings-storage", SettingsTab::Storage),
            ] {
                screens.push(case(
                    name,
                    false,
                    0,
                    Page::Settings,
                    ProfileTab::Outbounds,
                    PortForwardSubTab::Forwards,
                    AdvancedTab::Inbounds,
                    s_tab,
                ));
            }

            for sc in screens {
                let case_name = sc.name;
                app.ui.onboarding = sc.onboarding;
                app.ui.setup_step = sc.setup_step;
                app.ui.page = sc.page;
                app.ui.profile_tab = sc.profile_tab;
                app.ui.pf_subtab = sc.pf_subtab;
                app.ui.advanced_tab = sc.advanced_tab;
                app.ui.settings_tab = sc.settings_tab;
                let mut out = guided_frame(&mut app, &ctx, vec![], w, h);
                for _ in 0..3 {
                    out = guided_frame(&mut app, &ctx, vec![], w, h);
                }
                assert!(
                    overlapping_visible_text(&out).is_empty(),
                    "text overlap in {case_name} at {appearance:?} {w}x{h}: {:?}",
                    overlapping_visible_text(&out)
                );
                for shape in &out.shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        if text.galley.text().trim().is_empty() {
                            continue;
                        }
                        if matches!(
                            text.galley.text(),
                            "运行中"
                                | "已停止"
                                | "SSH 隧道"
                                | "TCP"
                                | "独立于 Core"
                                | "指纹已固定"
                                | "密码认证（已加密保存）"
                                | "常规分流规则"
                                | "默认访问"
                                | "已保存"
                                | "协议握手就绪"
                                | "可用 · 128 ms"
                        ) {
                            assert_eq!(
                                text.galley.rows.len(),
                                1,
                                "badge crushed in {case_name} at {appearance:?} {w}x{h}: {:?}",
                                text.galley.text()
                            );
                        }
                        for row in &text.galley.rows {
                            let rect = row
                                .visuals
                                .mesh_bounds
                                .translate(text.pos.to_vec2() + row.pos.to_vec2())
                                .intersect(shape.clip_rect);
                            if rect.width() > 2.0 && rect.height() > 2.0 {
                                assert!(
                                    rect.left() >= -0.5 && rect.right() <= w + 0.5,
                                    "horizontal text overflow in {case_name} at {appearance:?} {w}x{h}: {:?} rect={rect:?}",
                                    text.galley.text()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn sample_proxy_availability_fixture() -> ProxyAvailability {
    let steps = vec![
        DiagnosticStep {
            index: 1,
            name: "Core 进程与运行代次".into(),
            status: DiagnosticStepStatus::Pass,
            label: "通过".into(),
            detail: "配置「香港节点」运行中 · 冻结代次".into(),
            evidence: Some("generation-1".into()),
        },
        DiagnosticStep {
            index: 2,
            name: "本地代理入站握手".into(),
            status: DiagnosticStepStatus::Pass,
            label: "通过 · 4 ms".into(),
            detail: "入站 Tag：http-in (HTTP) · 无认证".into(),
            evidence: Some("127.0.0.1:10809".into()),
        },
        DiagnosticStep {
            index: 3,
            name: "路由规则归属（静态预测）".into(),
            status: DiagnosticStepStatus::Pass,
            label: "通过 · proxy".into(),
            detail: "默认走代理出站".into(),
            evidence: Some("expected_outbound=proxy".into()),
        },
        DiagnosticStep {
            index: 4,
            name: "经由代理端到端连通".into(),
            status: DiagnosticStepStatus::Pass,
            label: "可用 · 128 ms".into(),
            detail: "HTTP CONNECT + TLS · HTTP 204（本地 4 ms · TLS 42 ms · 总耗时 128 ms）".into(),
            evidence: Some(format!("target={DEFAULT_PROXY_PROBE_URL}")),
        },
        DiagnosticStep {
            index: 5,
            name: "系统代理 / PAC 生效关联".into(),
            status: DiagnosticStepStatus::Pass,
            label: "全局已接管".into(),
            detail: "系统代理指向本地入口 http-in".into(),
            evidence: None,
        },
    ];
    let history = (0..10)
        .map(|i| ProxyProbeSample {
            timestamp: format!("2026-11-23 15:0{}:00.000 +08:00", i % 10),
            state: if i == 3 {
                ProxyAvailabilityState::Degraded
            } else {
                ProxyAvailabilityState::Available
            },
            total_ms: Some(110 + i as u64 * 8),
            tls_ms: Some(36 + i as u64 * 2),
            http_status: Some(204),
            note: if i == 3 {
                "首次复测短暂抖动".into()
            } else {
                "自动检测正常".into()
            },
        })
        .collect();
    ProxyAvailability {
        generation_id: Some("generation-1".into()),
        runtime_instance_id: Some("instance".into()),
        state: ProxyAvailabilityState::Available,
        checking: false,
        badge_text: "可用 · 128 ms".into(),
        overview_hint: String::new(),
        headline: "代理链路畅通，经由代理出口可达外部站点".into(),
        target_url: DEFAULT_PROXY_PROBE_URL.into(),
        checked_at: Some("2026-11-23 15:09:00.000 +08:00".into()),
        local_ms: Some(4),
        tls_ms: Some(42),
        total_ms: Some(128),
        http_status: Some(204),
        inbound_tag: Some("http-in".into()),
        inbound_protocol: Some("http".into()),
        expected_outbound: Some("proxy".into()),
        fail_stage: None,
        fail_detail: None,
        consecutive_failures: 0,
        suggestions: vec![],
        steps,
        history,
        observatory: vec![ObservatoryOutboundStatus {
            outbound_tag: "proxy".into(),
            alive: true,
            delay_ms: Some(115),
            last_error: None,
        }],
        config: ProxyProbeConfig::default(),
        tray_throttled: false,
    }
}

#[test]
fn proxy_availability_overview_diagnostics_and_settings_flow() {
    let mut app = app();
    let (tx, requests) = mpsc::sync_channel(MAX_PENDING);
    app.tx = tx;
    let mut avail = sample_proxy_availability_fixture();
    app.status.as_mut().unwrap().proxy_availability = Some(avail.clone());

    let ctx = egui::Context::default();
    configure(&ctx);

    let find_text = |shape: &egui::Shape, pred: &dyn Fn(&str) -> bool| -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, pred: &dyn Fn(&str) -> bool) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if pred(t.galley.text()) => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| walk(s, pred)),
                _ => None,
            }
        }
        walk(shape, pred)
    };
    let collect_all_text = |out: &egui::FullOutput| -> Vec<String> {
        let mut texts = Vec::new();
        fn walk(shape: &egui::Shape, acc: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => acc.push(t.galley.text().to_owned()),
                egui::Shape::Vec(v) => {
                    for s in v {
                        walk(s, acc);
                    }
                }
                _ => {}
            }
        }
        for shape in &out.shapes {
            walk(&shape.shape, &mut texts);
        }
        texts
    };
    let click = |app: &mut App, label: &str| {
        guided_frame(app, &ctx, vec![], 1100.0, 900.0);
        let out = guided_frame(app, &ctx, vec![], 1100.0, 900.0);
        let pos = out
            .shapes
            .iter()
            .find_map(|s| find_text(&s.shape, &|t| t == label))
            .unwrap_or_else(|| panic!("missing clickable label: {label}"));
        guided_frame(
            app,
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            1100.0,
            900.0,
        );
        guided_frame(
            app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            1100.0,
            900.0,
        );
    };

    // 1. Overview healthy state: concise badge & actions, no verbose technical line.
    app.ui.page = Page::Overview;
    let ov = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    let ov_texts = collect_all_text(&ov);
    assert!(ov_texts.iter().any(|t| t == "代理可用性"));
    assert!(ov_texts.iter().any(|t| t == "可用 · 128 ms"));
    assert!(ov_texts.iter().any(|t| t == "重测"));
    assert!(ov_texts.iter().any(|t| t == "诊断详情"));
    assert!(!ov_texts.iter().any(|t| t.contains("经由 http-in")));
    assert!(!ov_texts.iter().any(|t| t.contains("自动检测")));

    click(&mut app, "重测");
    assert!(requests.try_iter().any(|r| matches!(
        r.command,
        Command::CheckProxyAvailability { target_url: None }
    )));
    app.busy = false;

    // 2. Overview abnormal states: DirectOnly & Unavailable alert suffixes and short hints.
    avail.state = ProxyAvailabilityState::DirectOnly;
    avail.badge_text = "直连可达（未走代理）".into();
    avail.overview_hint = "当前检测地址未走代理，建议更换".into();
    app.status.as_mut().unwrap().proxy_availability = Some(avail.clone());
    let direct_ov = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    let direct_texts = collect_all_text(&direct_ov);
    assert!(direct_texts.iter().any(|t| t.contains("· 未走代理")));
    assert!(
        direct_texts
            .iter()
            .any(|t| t == "当前检测地址未走代理，建议更换")
    );

    avail.state = ProxyAvailabilityState::Unavailable;
    avail.badge_text = "不可用".into();
    avail.overview_hint = "本地入口正常，远端节点连接失败".into();
    app.status.as_mut().unwrap().proxy_availability = Some(avail);
    let unavail_ov = guided_frame(&mut app, &ctx, vec![], 1100.0, 900.0);
    let unavail_texts = collect_all_text(&unavail_ov);
    assert!(unavail_texts.iter().any(|t| t.contains("· 代理不可用")));
    assert!(
        unavail_texts
            .iter()
            .any(|t| t == "本地入口正常，远端节点连接失败")
    );

    // Clicking 诊断详情 opens Diagnostics page.
    click(&mut app, "诊断详情");
    assert_eq!(app.ui.page, Page::Diagnostics);

    // 3. Diagnostics page: 5 steps + ad-hoc URL test + redacted Markdown report.
    app.status.as_mut().unwrap().proxy_availability = Some(sample_proxy_availability_fixture());
    let diag = guided_frame(&mut app, &ctx, vec![], 1100.0, 1500.0);
    let diag_texts = collect_all_text(&diag);
    assert!(diag_texts.iter().any(|t| t == "代理可用性诊断报告"));
    assert!(diag_texts.iter().any(|t| t == "Core 进程与运行代次"));
    assert!(diag_texts.iter().any(|t| t == "系统代理 / PAC 生效关联"));
    assert!(
        diag_texts
            .iter()
            .any(|t| t == "近期稳定性与延迟曲线（最近 10 次）")
    );

    click(&mut app, "gstatic HTTPS 204");
    click(&mut app, "测试此地址");
    assert!(requests.try_iter().any(|r| matches!(
        r.command,
        Command::CheckProxyAvailability { target_url: Some(ref u) }
            if u == "https://www.gstatic.com/generate_204"
    )));
    app.busy = false;

    let report = app.format_proxy_diagnostic_report();
    assert!(report.contains("# Proxy Pilot 代理可用性诊断报告"));
    assert!(report.contains("## 诊断结论"));
    assert!(report.contains("## 五步链路核查"));
    assert!(report.contains("## Xray Native Observatory（只读）"));
    assert!(report.contains("## 近期探测样本（最近 5 条）"));
    assert!(!report.contains("Proxy-Authorization"));
    assert!(!report.contains("password"));

    // 4. Settings -> 代理探测 tab saves ProxyProbeConfig independently of SaveSettings.
    app.ui.page = Page::Settings;
    click(&mut app, "代理探测");
    assert_eq!(app.ui.settings_tab, SettingsTab::Probe);
    click(&mut app, "Cloudflare HTTP 204（免 TLS）");
    click(&mut app, "保存设置（热生效）");
    let cmds: Vec<Command> = requests.try_iter().map(|r| r.command).collect();
    assert!(cmds.iter().any(|c| matches!(
        c,
        Command::SaveProxyProbeConfig { config }
            if config.target_url == "http://cp.cloudflare.com/generate_204"
    )));
    assert!(
        !cmds
            .iter()
            .any(|c| matches!(c, Command::SaveSettings { .. }))
    );
}
