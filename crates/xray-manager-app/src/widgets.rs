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
