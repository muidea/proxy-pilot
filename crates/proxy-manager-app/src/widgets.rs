use eframe::egui;

pub const CONTROL_HEIGHT: f32 = 36.0;

/// A single-line field uses the same outer height and text alignment as buttons.
/// `width` includes the horizontal padding; infinity fills the available cell.
pub fn singleline(value: &mut String, width: f32) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(value)
        .desired_width((width - 20.0).max(24.0))
        .min_size(egui::vec2(0.0, CONTROL_HEIGHT))
        .margin(egui::Margin::symmetric(10, 8))
        .vertical_align(egui::Align::Center)
}

/// Selects all existing characters inside a `TextEdit` so the next keystroke replaces them.
pub fn select_all_text(
    ctx: &egui::Context,
    mut state: egui::text_edit::TextEditState,
    id: egui::Id,
    char_len: usize,
) {
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(char_len),
        )));
    state.store(ctx, id);
}

/// A single-line input that automatically selects its existing text whenever focused.
pub fn singleline_select_all(
    ui: &mut egui::Ui,
    value: &mut String,
    width: f32,
    hint: &str,
) -> egui::Response {
    let char_len = value.chars().count();
    let output = singleline(value, width).hint_text(hint).show(ui);
    if output.response.gained_focus() && char_len > 0 {
        select_all_text(ui.ctx(), output.state, output.response.id, char_len);
    }
    output.response
}

/// Re-focuses `response` and selects all existing text so the next keystroke replaces it.
pub fn focus_and_select_all(ctx: &egui::Context, response: &egui::Response, value: &str) {
    response.request_focus();
    if let Some(state) = egui::TextEdit::load_state(ctx, response.id) {
        select_all_text(ctx, state, response.id, value.chars().count());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    Accent,
    Success,
    Warn,
    Danger,
    Muted,
}

pub fn accent_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(77, 151, 255)
    } else {
        egui::Color32::from_rgb(29, 111, 232)
    }
}

pub fn accent_soft_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgba_unmultiplied(77, 151, 255, 41)
    } else {
        egui::Color32::from_rgba_unmultiplied(29, 111, 232, 28)
    }
}

pub fn panel_alt_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(30, 40, 55)
    } else {
        egui::Color32::from_rgb(248, 250, 253)
    }
}

pub fn border_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(43, 55, 73)
    } else {
        egui::Color32::from_rgb(223, 229, 239)
    }
}

pub fn border_strong(dark: bool) -> egui::Color32 {
    if dark {
        egui::Color32::from_rgb(60, 76, 99)
    } else {
        egui::Color32::from_rgb(197, 208, 224)
    }
}

pub fn border_strong_color(ui: &egui::Ui) -> egui::Color32 {
    border_strong(ui.visuals().dark_mode)
}

pub fn tone_palette(
    ui: &egui::Ui,
    tone: BadgeTone,
) -> (egui::Color32, egui::Color32, egui::Color32) {
    let dark = ui.visuals().dark_mode;
    match tone {
        BadgeTone::Accent => {
            let fg = accent_color(ui);
            let bg = accent_soft_color(ui);
            (fg, bg, fg.linear_multiply(0.32))
        }
        BadgeTone::Success => {
            let fg = if dark {
                egui::Color32::from_rgb(74, 222, 128)
            } else {
                egui::Color32::from_rgb(21, 128, 61)
            };
            let bg = if dark {
                egui::Color32::from_rgba_unmultiplied(74, 222, 128, 36)
            } else {
                egui::Color32::from_rgba_unmultiplied(21, 128, 61, 30)
            };
            (fg, bg, fg.linear_multiply(0.32))
        }
        BadgeTone::Warn => {
            let fg = if dark {
                egui::Color32::from_rgb(251, 191, 36)
            } else {
                egui::Color32::from_rgb(180, 83, 9)
            };
            let bg = if dark {
                egui::Color32::from_rgba_unmultiplied(251, 191, 36, 36)
            } else {
                egui::Color32::from_rgba_unmultiplied(217, 119, 6, 32)
            };
            (fg, bg, fg.linear_multiply(0.32))
        }
        BadgeTone::Danger => {
            let fg = if dark {
                egui::Color32::from_rgb(248, 113, 113)
            } else {
                egui::Color32::from_rgb(185, 28, 28)
            };
            let bg = if dark {
                egui::Color32::from_rgba_unmultiplied(248, 113, 113, 40)
            } else {
                egui::Color32::from_rgba_unmultiplied(220, 38, 38, 28)
            };
            (fg, bg, fg.linear_multiply(0.32))
        }
        BadgeTone::Muted => {
            let fg = ui.visuals().text_color().linear_multiply(0.76);
            let bg = panel_alt_color(ui);
            let border = border_color(ui);
            (fg, bg, border)
        }
    }
}

pub fn status_dot(ui: &mut egui::Ui, running: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let center = rect.center();
        let (dot_color, _, _) = tone_palette(
            ui,
            if running {
                BadgeTone::Success
            } else {
                BadgeTone::Muted
            },
        );
        if running {
            ui.painter()
                .circle_filled(center, 7.0, dot_color.linear_multiply(0.22));
        }
        ui.painter().circle_filled(
            center,
            4.5,
            if running {
                dot_color
            } else {
                ui.visuals().weak_text_color()
            },
        );
    }
}

pub fn nav_tab(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let accent = accent_color(ui);
    let fill = if active {
        accent_soft_color(ui)
    } else {
        egui::Color32::TRANSPARENT
    };
    let stroke = if active {
        egui::Stroke::new(1.0, accent.linear_multiply(0.3))
    } else {
        egui::Stroke::NONE
    };
    let text = if active {
        egui::RichText::new(label).strong().color(accent)
    } else {
        egui::RichText::new(label).color(ui.visuals().weak_text_color())
    };
    ui.add(
        egui::Button::new(text)
            .fill(fill)
            .stroke(stroke)
            .corner_radius(8)
            .min_size(egui::vec2(0.0, 32.0)),
    )
}

pub fn pill_tab(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let accent = accent_color(ui);
    let fill = if active { accent } else { panel_alt_color(ui) };
    let stroke = egui::Stroke::new(1.0, if active { accent } else { border_color(ui) });
    let text = if active {
        egui::RichText::new(label)
            .strong()
            .color(egui::Color32::WHITE)
    } else {
        egui::RichText::new(label).color(ui.visuals().weak_text_color())
    };
    ui.add(
        egui::Button::new(text)
            .fill(fill)
            .stroke(stroke)
            .corner_radius(8),
    )
}

pub fn seg_button(ui: &mut egui::Ui, label: &str, active: bool, enabled: bool) -> egui::Response {
    let accent = accent_color(ui);
    let fill = if active {
        accent
    } else {
        ui.visuals().window_fill()
    };
    let stroke = egui::Stroke::new(
        1.0,
        if active {
            accent
        } else {
            border_strong_color(ui)
        },
    );
    let text = if active {
        egui::RichText::new(label)
            .small()
            .strong()
            .color(egui::Color32::WHITE)
    } else {
        egui::RichText::new(label)
            .small()
            .color(ui.visuals().weak_text_color())
    };
    ui.add_enabled(
        enabled,
        egui::Button::new(text)
            .fill(fill)
            .stroke(stroke)
            .corner_radius(7),
    )
}

/// Compact bordered button for dense list rows where 36 px controls would clip or inflate rows.
pub fn compact_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    compact_chip(ui, label, false)
}

/// Compact bordered button with explicit enabled state for dense header or card action rows.
pub fn compact_button_enabled(ui: &mut egui::Ui, enabled: bool, label: &str) -> egui::Response {
    compact_chip_enabled(ui, enabled, label, false)
}

/// Compact 24 px selectable pill/chip with explicit enabled state for dense headers and mode rows.
pub fn compact_chip_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    active: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| compact_chip(ui, label, active))
        .inner
}

/// Truncates `text` to `max_chars` characters, appending an ellipsis only when shortened.
pub fn ellipsize(text: &str, max_chars: usize) -> String {
    let mut iter = text.chars();
    let prefix: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn responsive_sides_header(
    ui: &mut egui::Ui,
    min_inline_width: f32,
    height: Option<f32>,
    left: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui, bool),
) {
    let apply_height = |ui: &mut egui::Ui| {
        if let Some(h) = height {
            ui.spacing_mut().interact_size.y = (h - 2.0).max(20.0);
        }
    };
    if ui.available_width() >= min_inline_width {
        let mut sides = egui::Sides::new().shrink_left().wrap();
        if let Some(h) = height {
            sides = sides.height(h);
        }
        sides.show(
            ui,
            |ui| {
                apply_height(ui);
                left(ui);
            },
            |ui| {
                apply_height(ui);
                right(ui, true);
            },
        );
    } else {
        ui.scope(|ui| {
            apply_height(ui);
            left(ui);
            ui.horizontal_wrapped(|ui| right(ui, false));
        });
    }
}

/// Renders a two-sided header inline above `min_inline_width`, stacking actions below on narrower widths.
pub fn responsive_header(
    ui: &mut egui::Ui,
    min_inline_width: f32,
    left: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui, bool),
) {
    responsive_sides_header(ui, min_inline_width, None, left, right);
}

/// Compact variant of `responsive_header` tuned for 24 px compact controls inside overview/dense cards.
pub fn responsive_compact_header(
    ui: &mut egui::Ui,
    min_inline_width: f32,
    left: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui, bool),
) {
    responsive_sides_header(ui, min_inline_width, Some(26.0), left, right);
}

/// Compact 24 px selectable pill/chip for dense quick-preset rows inside modals and lists.
pub fn compact_chip(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let accent = accent_color(ui);
    let fill = if active {
        accent
    } else {
        ui.visuals().window_fill()
    };
    let stroke = egui::Stroke::new(
        1.0,
        if active {
            accent
        } else {
            border_strong_color(ui)
        },
    );
    let text = if active {
        egui::RichText::new(label)
            .small()
            .strong()
            .color(egui::Color32::WHITE)
    } else {
        egui::RichText::new(label).small()
    };
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
        ui.add(
            egui::Button::new(text)
                .fill(fill)
                .stroke(stroke)
                .corner_radius(6)
                .min_size(egui::vec2(0.0, 24.0)),
        )
    })
    .inner
}

fn subcard_with_style<R>(
    ui: &mut egui::Ui,
    border_tone: Option<BadgeTone>,
    dimmed: bool,
    corner_radius: u8,
    margin: egui::Margin,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let fill = if dimmed {
        panel_alt_color(ui)
    } else {
        ui.visuals().window_fill()
    };
    let border = match border_tone {
        Some(tone) => tone_palette(ui, tone).0,
        None => border_color(ui),
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(corner_radius)
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            body(ui)
        })
        .inner
}

pub fn subcard<R>(
    ui: &mut egui::Ui,
    border_tone: Option<BadgeTone>,
    dimmed: bool,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    subcard_with_style(
        ui,
        border_tone,
        dimmed,
        10,
        egui::Margin::symmetric(14, 11),
        body,
    )
}

/// Compact subcard variant for dense Overview tunnel items and secondary nested blocks.
pub fn compact_subcard<R>(
    ui: &mut egui::Ui,
    border_tone: Option<BadgeTone>,
    dimmed: bool,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    subcard_with_style(
        ui,
        border_tone,
        dimmed,
        8,
        egui::Margin::symmetric(12, 9),
        body,
    )
}

pub fn kv_row_singleline(ui: &mut egui::Ui, key: &str, value: egui::RichText, hover: Option<&str>) {
    let label_width = 104.0_f32.min(ui.available_width() * 0.34);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 0.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(label_width);
                ui.set_max_width(label_width);
                ui.add(egui::Label::new(egui::RichText::new(key).small().weak()).truncate());
            },
        );
        let resp = ui.add(egui::Label::new(value).truncate());
        if let Some(tip) = hover.filter(|t| !t.is_empty()) {
            resp.on_hover_text(tip);
        }
    });
}

fn render_badge(ui: &mut egui::Ui, text: &str, tone: BadgeTone, monospace: bool) {
    let (fg, bg, border) = tone_palette(ui, tone);
    let mut rich = egui::RichText::new(text).small().strong().color(fg);
    if monospace {
        rich = rich.monospace();
    }
    let max_text_w = if ui.max_rect().width().is_finite() {
        (ui.max_rect().width() - 18.0).max(48.0)
    } else {
        f32::INFINITY
    };
    let galley = egui::WidgetText::from(rich).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        max_text_w,
        egui::TextStyle::Small,
    );
    let pad = egui::vec2(8.0, 3.0);
    let desired = galley.size() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let radius = if galley.rows.len() <= 1 {
            egui::CornerRadius::same(255)
        } else {
            egui::CornerRadius::same(10)
        };
        ui.painter().rect(
            rect,
            radius,
            bg,
            egui::Stroke::new(1.0, border),
            egui::StrokeKind::Inside,
        );
        let text_pos = egui::pos2(rect.min.x + pad.x, rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(text_pos, galley, fg);
    }
}

pub fn badge(ui: &mut egui::Ui, text: &str, tone: BadgeTone) {
    render_badge(ui, text, tone, false);
}

pub fn mono_badge(ui: &mut egui::Ui, text: &str, tone: BadgeTone) {
    render_badge(ui, text, tone, true);
}

pub fn clickable_mono_badge(ui: &mut egui::Ui, text: &str, tone: BadgeTone) -> egui::Response {
    let (fg, bg, border) = tone_palette(ui, tone);
    let rich = egui::RichText::new(text)
        .small()
        .strong()
        .monospace()
        .color(fg);
    ui.add(
        egui::Button::new(rich)
            .fill(bg)
            .stroke(egui::Stroke::new(1.0, border))
            .corner_radius(egui::CornerRadius::same(255))
            .min_size(egui::Vec2::ZERO),
    )
}

pub fn banner_box(ui: &mut egui::Ui, tone: BadgeTone, text: &str) {
    let (fg, bg, border) = tone_palette(ui, tone);
    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.add(egui::Label::new(egui::RichText::new(text).small().color(fg)).wrap());
        });
}

pub fn flow_strip(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(body);
        });
}
