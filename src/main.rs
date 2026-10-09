// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app;
mod config;
mod node;
mod ui;

use std::sync::Arc;

use app::FlorestaApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Floresta")
            .with_inner_size([1180.0, 780.0])
            .with_icon(app_icon())
            .with_min_inner_size([680.0, 560.0]),
        renderer: eframe::Renderer::Glow,
        persist_window: true,
        ..Default::default()
    };

    eframe::run_native(
        "Floresta",
        options,
        Box::new(|creation| Ok(Box::new(FlorestaApp::new(creation)))),
    )
}

fn app_icon() -> Arc<egui::IconData> {
    const SIZE: usize = 64;
    let mut rgba = vec![0_u8; SIZE * SIZE * 4];
    fill_icon_rect(&mut rgba, 8, 8, 18, 18, [5, 143, 91, 255]);
    fill_icon_rect(&mut rgba, 24, 16, 18, 18, [5, 143, 91, 255]);
    fill_icon_rect(&mut rgba, 24, 32, 18, 28, [21, 30, 25, 255]);
    fill_icon_rect(&mut rgba, 40, 32, 20, 18, [21, 30, 25, 255]);
    Arc::new(egui::IconData {
        rgba,
        width: SIZE as u32,
        height: SIZE as u32,
    })
}

fn fill_icon_rect(
    rgba: &mut [u8],
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    const SIZE: usize = 64;
    for y in top..top + height {
        for x in left..left + width {
            let offset = (y * SIZE + x) * 4;
            rgba[offset..offset + 4].copy_from_slice(&color);
        }
    }
}
