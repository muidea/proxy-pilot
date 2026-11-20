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
    let text = egui::RichText::new(label).small();
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
        ui.add(
            egui::Button::new(text)
                .fill(ui.visuals().window_fill())
                .stroke(egui::Stroke::new(1.0, border_strong_color(ui)))
                .corner_radius(6)
                .min_size(egui::vec2(0.0, 24.0)),
        )
    })
    .inner
}

pub fn subcard<R>(
    ui: &mut egui::Ui,
    border_tone: Option<BadgeTone>,
    dimmed: bool,
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
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 11))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            body(ui)
        })
        .inner
}

pub fn kv_row(ui: &mut egui::Ui, key: &str, value: egui::RichText) {
    let label_width = 148.0_f32.min(ui.available_width() * 0.42);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_width(label_width);
                ui.add(egui::Label::new(egui::RichText::new(key).small().weak()).wrap());
            },
        );
        ui.add(egui::Label::new(value).wrap());
    });
}

fn render_badge(ui: &mut egui::Ui, text: &str, tone: BadgeTone, monospace: bool) {
    let (fg, bg, border) = tone_palette(ui, tone);
    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            let mut rich = egui::RichText::new(text).small().strong().color(fg);
            if monospace {
                rich = rich.monospace();
            }
            ui.label(rich);
        });
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
