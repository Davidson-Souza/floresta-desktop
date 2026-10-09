// SPDX-License-Identifier: MIT OR Apache-2.0

use eframe::egui;
use egui::Align;
use egui::Color32;
use egui::CornerRadius;
use egui::InnerResponse;
use egui::Layout;
use egui::Response;
use egui::RichText;
use egui::Sense;
use egui::Stroke;
use egui::TextStyle;
use egui::Ui;
use egui::Vec2;
use egui::WidgetInfo;
use egui::WidgetType;

use super::theme;

pub fn brand(ui: &mut Ui, compact: bool) {
    ui.horizontal(|ui| {
        let size = if compact { 23.0 } else { 28.0 };
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        let painter = ui.painter();
        let unit = size / 7.0;
        let square = |x: f32, y: f32, width: f32, height: f32| {
            egui::Rect::from_min_size(
                rect.min + egui::vec2(x * unit, y * unit),
                egui::vec2(width * unit, height * unit),
            )
        };
        painter.rect_filled(square(0.0, 0.0, 2.0, 2.0), 1.0, theme::green());
        painter.rect_filled(square(2.0, 1.0, 2.0, 2.0), 1.0, theme::green());
        painter.rect_filled(square(2.0, 3.0, 2.0, 4.0), 1.0, theme::text());
        painter.rect_filled(square(4.0, 3.0, 3.0, 2.0), 1.0, theme::text());

        ui.add_space(2.0);
        ui.label(
            RichText::new("Floresta")
                .size(if compact { 20.0 } else { 24.0 })
                .color(theme::text())
                .strong(),
        );
    });
}

pub fn card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    egui::Frame::new()
        .fill(theme::surface())
        .stroke(Stroke::new(1.0_f32, theme::border()))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(20))
        .show(ui, add_contents)
}

pub fn muted_card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    egui::Frame::new()
        .fill(theme::surface_muted())
        .stroke(Stroke::new(1.0_f32, theme::border()))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(16))
        .show(ui, add_contents)
}

pub fn primary_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text.into()).color(Color32::WHITE).strong())
            .fill(theme::button_green())
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(7))
            .min_size(egui::vec2(124.0, 42.0)),
    )
}

pub fn secondary_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text.into()).color(theme::text()).strong())
            .fill(theme::surface())
            .stroke(Stroke::new(1.0_f32, theme::border()))
            .corner_radius(CornerRadius::same(7))
            .min_size(egui::vec2(96.0, 42.0)),
    )
}

pub fn text_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(
        egui::Button::new(
            RichText::new(text.into())
                .color(theme::green_dark())
                .strong(),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE),
    )
}

pub fn toggle(ui: &mut Ui, value: &mut bool) -> Response {
    let desired_size = egui::vec2(43.0, 24.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let fill = if *value {
            theme::green()
        } else {
            theme::toggle_off()
        };
        ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
        let center_x = if *value {
            rect.right() - 12.0
        } else {
            rect.left() + 12.0
        };
        ui.painter()
            .circle_filled(egui::pos2(center_x, rect.center().y), 9.0, Color32::WHITE);
    }

    response
}

pub fn toggle_setting(ui: &mut Ui, value: &mut bool, title: &str, description: &str) -> Response {
    let mut changed = None;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).color(theme::text()).strong());
            ui.label(RichText::new(description).color(theme::text_muted()));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed = Some(toggle(ui, value));
        });
    });
    let response = changed.expect("toggle row always creates a response");
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), *value, title));
    response
}

pub fn progress_bar(ui: &mut Ui, progress: f32) -> Response {
    let progress = progress.clamp(0.0, 1.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.0), 9.0),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, CornerRadius::same(5), theme::surface_muted());
    if progress > 0.0 {
        let fill =
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * progress, rect.height()));
        ui.painter()
            .rect_filled(fill, CornerRadius::same(5), theme::green());
    }
    response.widget_info(|| {
        let mut info = WidgetInfo::new(WidgetType::ProgressIndicator);
        info.value = Some(f64::from(progress * 100.0));
        info
    });
    response
}

pub fn page_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(
        RichText::new(title)
            .text_style(TextStyle::Name("Page title".into()))
            .color(theme::text())
            .strong(),
    );
    ui.label(
        RichText::new(subtitle)
            .size(16.0)
            .color(theme::text_muted()),
    );
}

pub fn card_title(ui: &mut Ui, title: &str) {
    ui.label(
        RichText::new(title)
            .text_style(TextStyle::Name("Card title".into()))
            .color(theme::text())
            .strong(),
    );
}

pub fn metric(ui: &mut Ui, value: impl Into<String>, label: &str) {
    ui.label(
        RichText::new(value.into())
            .text_style(TextStyle::Name("Metric".into()))
            .color(theme::text())
            .strong(),
    );
    ui.label(RichText::new(label).color(theme::text_muted()));
}

pub fn inline_error(ui: &mut Ui, message: &str) {
    egui::Frame::new()
        .fill(theme::red_pale())
        .stroke(Stroke::new(1.0_f32, theme::red().linear_multiply(0.35)))
        .corner_radius(CornerRadius::same(7))
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.label(RichText::new(message).color(theme::red()));
        });
}

pub fn status_pill(ui: &mut Ui, label: &str, color: Color32, background: Color32) {
    egui::Frame::new()
        .fill(background)
        .corner_radius(CornerRadius::same(20))
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(12.0).color(color).strong());
        });
}

pub fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::border());
}
