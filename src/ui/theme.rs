// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::Cell;

use eframe::egui;
use egui::Color32;
use egui::FontFamily;
use egui::FontId;
use egui::Stroke;
use egui::TextStyle;

thread_local! {
    static DARK_MODE: Cell<bool> = const { Cell::new(false) };
}

pub fn is_dark() -> bool {
    DARK_MODE.get()
}

fn color(light: Color32, dark: Color32) -> Color32 {
    if is_dark() { dark } else { light }
}

pub fn background() -> Color32 {
    color(
        Color32::from_rgb(246, 248, 247),
        Color32::from_rgb(11, 16, 13),
    )
}

pub fn surface() -> Color32 {
    color(
        Color32::from_rgb(255, 255, 255),
        Color32::from_rgb(18, 26, 22),
    )
}

pub fn surface_muted() -> Color32 {
    color(
        Color32::from_rgb(240, 245, 242),
        Color32::from_rgb(25, 37, 31),
    )
}

pub fn border() -> Color32 {
    color(
        Color32::from_rgb(218, 226, 221),
        Color32::from_rgb(49, 64, 56),
    )
}

pub fn text() -> Color32 {
    color(
        Color32::from_rgb(21, 30, 25),
        Color32::from_rgb(235, 242, 238),
    )
}

pub fn text_muted() -> Color32 {
    color(
        Color32::from_rgb(96, 109, 102),
        Color32::from_rgb(155, 171, 162),
    )
}

pub fn green() -> Color32 {
    color(
        Color32::from_rgb(5, 143, 91),
        Color32::from_rgb(34, 197, 126),
    )
}

pub fn button_green() -> Color32 {
    Color32::from_rgb(3, 130, 81)
}

pub fn green_dark() -> Color32 {
    color(
        Color32::from_rgb(3, 112, 71),
        Color32::from_rgb(83, 220, 157),
    )
}

pub fn green_pale() -> Color32 {
    color(
        Color32::from_rgb(228, 246, 237),
        Color32::from_rgb(20, 55, 41),
    )
}

pub fn amber() -> Color32 {
    color(
        Color32::from_rgb(183, 113, 19),
        Color32::from_rgb(238, 178, 89),
    )
}

pub fn amber_pale() -> Color32 {
    color(
        Color32::from_rgb(255, 246, 224),
        Color32::from_rgb(59, 43, 20),
    )
}

pub fn red() -> Color32 {
    color(
        Color32::from_rgb(183, 52, 65),
        Color32::from_rgb(242, 119, 131),
    )
}

pub fn red_pale() -> Color32 {
    color(
        Color32::from_rgb(255, 235, 237),
        Color32::from_rgb(61, 28, 34),
    )
}

pub fn blue() -> Color32 {
    color(
        Color32::from_rgb(39, 104, 176),
        Color32::from_rgb(113, 178, 244),
    )
}

pub fn blue_pale() -> Color32 {
    color(
        Color32::from_rgb(232, 241, 252),
        Color32::from_rgb(23, 43, 64),
    )
}

pub fn toggle_off() -> Color32 {
    color(
        Color32::from_rgb(190, 199, 194),
        Color32::from_rgb(84, 99, 90),
    )
}

fn extreme_background() -> Color32 {
    color(
        Color32::from_rgb(235, 240, 237),
        Color32::from_rgb(9, 14, 11),
    )
}

pub fn apply(ctx: &egui::Context, dark_mode: bool) {
    DARK_MODE.set(dark_mode);
    ctx.set_theme(if dark_mode {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });

    let mut style = (*ctx.style()).clone();
    style.visuals = if dark_mode {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    style.visuals.override_text_color = Some(text());
    style.visuals.panel_fill = background();
    style.visuals.window_fill = surface();
    style.visuals.faint_bg_color = surface_muted();
    style.visuals.extreme_bg_color = extreme_background();
    style.visuals.hyperlink_color = blue();
    style.visuals.warn_fg_color = amber();
    style.visuals.error_fg_color = red();
    style.visuals.selection.bg_fill = green_pale();
    style.visuals.selection.stroke = Stroke::new(1.0_f32, green());
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text());
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, text_muted());
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, text());
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, text());
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.interact_size.y = 40.0;
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(27.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Name("Page title".into()),
        FontId::new(30.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Name("Card title".into()),
        FontId::new(17.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Name("Metric".into()),
        FontId::new(30.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(15.0, FontFamily::Proportional),
    );
    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_theme_changes_every_semantic_surface() {
        let ctx = egui::Context::default();
        apply(&ctx, false);
        let light = [
            background(),
            surface(),
            surface_muted(),
            text(),
            text_muted(),
            green_pale(),
        ];

        apply(&ctx, true);
        let dark = [
            background(),
            surface(),
            surface_muted(),
            text(),
            text_muted(),
            green_pale(),
        ];

        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert!(is_dark());
        assert!(light.iter().zip(dark).all(|(light, dark)| *light != dark));
        assert!(luminance(background()) < luminance(light[0]));
    }

    fn luminance(color: Color32) -> u16 {
        u16::from(color.r()) + u16::from(color.g()) + u16::from(color.b())
    }
}
