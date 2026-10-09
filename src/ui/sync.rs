// SPDX-License-Identifier: MIT OR Apache-2.0

use std::f32::consts::TAU;
use std::time::Duration;

use eframe::egui;
use egui::Align;
use egui::Color32;
use egui::CornerRadius;
use egui::Layout;
use egui::RichText;
use egui::Sense;
use egui::Stroke;
use egui::Ui;

use super::components;
use super::theme;
use crate::config::NodeConfig;
use crate::node::NodeSnapshot;

#[derive(Clone, Copy)]
pub enum SyncStatus<'a> {
    Starting(&'a str),
    Syncing(&'a NodeSnapshot),
    Ready(&'a NodeSnapshot),
    Error(&'a str),
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncAction {
    None,
    OpenDashboard,
    OpenActivity,
    Retry,
    EditSetup,
}

pub fn show(
    ctx: &egui::Context,
    config: &NodeConfig,
    status: SyncStatus<'_>,
    dark_mode: &mut bool,
) -> SyncAction {
    let mut action = SyncAction::None;
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::background())
                .inner_margin(egui::Margin::symmetric(24, 20)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        ui.set_max_width(900.0);
                        top_bar(ui, ctx, config, dark_mode);
                        ui.add_space(42.0);

                        match status {
                            SyncStatus::Starting(detail) => starting(ui, ctx, detail),
                            SyncStatus::Syncing(snapshot) => syncing(ui, config, snapshot),
                            SyncStatus::Ready(snapshot) => {
                                action = ready(ui, config, snapshot);
                            }
                            SyncStatus::Error(message) => action = error(ui, message),
                            SyncStatus::Stopped => action = stopped(ui),
                        }
                    });
                });
        });
    action
}

fn top_bar(ui: &mut Ui, ctx: &egui::Context, config: &NodeConfig, dark_mode: &mut bool) {
    ui.horizontal(|ui| {
        components::brand(ui, true);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            components::status_pill(
                ui,
                config.network.title(),
                theme::green_dark(),
                theme::green_pale(),
            );
            if config.use_tor {
                components::status_pill(ui, "TOR", theme::blue(), theme::blue_pale());
            }
            let label = if *dark_mode {
                "Light mode"
            } else {
                "Dark mode"
            };
            if components::text_button(ui, label).clicked() {
                *dark_mode = !*dark_mode;
                theme::apply(ctx, *dark_mode);
            }
        });
    });
}

fn starting(ui: &mut Ui, ctx: &egui::Context, detail: &str) {
    spinner(ui, ctx);
    ui.add_space(24.0);
    ui.label(
        RichText::new("Starting your node")
            .size(30.0)
            .color(theme::text())
            .strong(),
    );
    ui.label(
        RichText::new("Preparing a private connection to the Bitcoin network.")
            .size(16.0)
            .color(theme::text_muted()),
    );
    ui.add_space(24.0);
    components::muted_card(ui, |ui| {
        ui.horizontal(|ui| {
            pulse_dot(ui, ctx);
            ui.label(RichText::new(detail).color(theme::text()).strong());
        });
    });
    ui.add_space(18.0);
    ui.label(
        RichText::new("This normally takes a few moments. Your settings are already saved.")
            .size(13.0)
            .color(theme::text_muted()),
    );
}

fn syncing(ui: &mut Ui, config: &NodeConfig, snapshot: &NodeSnapshot) {
    let progress = snapshot.verification_progress;
    ui.label(
        RichText::new(snapshot.phase().to_uppercase())
            .size(12.0)
            .color(theme::green_dark())
            .strong(),
    );
    ui.label(
        RichText::new(format!("{:.1}%", progress * 100.0))
            .size(48.0)
            .color(theme::text())
            .strong(),
    );
    ui.label(
        RichText::new("Your node is independently checking Bitcoin's history.")
            .size(16.0)
            .color(theme::text_muted()),
    );
    ui.add_space(20.0);
    components::progress_bar(ui, progress);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Block {}", format_number(snapshot.block_height)))
                .color(theme::text())
                .strong(),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!(
                    "{} remaining",
                    format_number(snapshot.blocks_behind())
                ))
                .color(theme::text_muted()),
            );
        });
    });

    ui.add_space(24.0);
    if ui.available_width() >= 680.0 {
        ui.columns(3, |columns| {
            sync_metric(
                &mut columns[0],
                "Peers",
                snapshot.peer_count.to_string(),
                "Connected now",
            );
            sync_metric(
                &mut columns[1],
                "Chain target",
                format_number(snapshot.target_height),
                "Headers known",
            );
            sync_metric(
                &mut columns[2],
                "Stored",
                format_bytes(snapshot.size_on_disk),
                "Compact chain state",
            );
        });
    } else {
        sync_metric(
            ui,
            "Peers",
            snapshot.peer_count.to_string(),
            "Connected now",
        );
        ui.add_space(10.0);
        sync_metric(
            ui,
            "Chain target",
            format_number(snapshot.target_height),
            "Headers known",
        );
        ui.add_space(10.0);
        sync_metric(
            ui,
            "Stored",
            format_bytes(snapshot.size_on_disk),
            "Compact chain state",
        );
    }

    ui.add_space(20.0);
    if snapshot.peer_count == 0 {
        egui::Frame::new()
            .fill(theme::amber_pale())
            .stroke(Stroke::new(1.0_f32, theme::amber().linear_multiply(0.3)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Waiting for peers")
                        .color(theme::amber())
                        .strong(),
                );
                ui.label(
                    RichText::new(if config.use_tor {
                        "Floresta is waiting for Tor to provide a route. Check that Tor is running if this persists."
                    } else {
                        "Floresta is looking for trustworthy peers. No action is usually needed."
                    })
                    .color(theme::text()),
                );
            });
    } else {
        components::muted_card(ui, |ui| {
            ui.label(
                RichText::new("You can leave Floresta open")
                    .color(theme::text())
                    .strong(),
            );
            ui.label(
                RichText::new(
                    "Progress is saved continuously. Closing the app pauses the node and resumes safely next time.",
                )
                .color(theme::text_muted()),
            );
        });
    }

    if config.fast_start && config.backfill {
        ui.add_space(12.0);
        components::muted_card(ui, |ui| {
            ui.label(
                RichText::new("Background validation scheduled")
                    .color(theme::text())
                    .strong(),
            );
            ui.label(
                RichText::new(
                    "After fast start, Floresta will validate older blocks in the background. Exact backfill progress is not exposed by this node version.",
                )
                .color(theme::text_muted()),
            );
        });
    }

    if snapshot.received_at.elapsed() > Duration::from_secs(8) {
        ui.add_space(12.0);
        ui.label(
            RichText::new("Status is taking longer than expected to refresh.")
                .size(13.0)
                .color(theme::amber()),
        );
    }
}

fn ready(ui: &mut Ui, config: &NodeConfig, snapshot: &NodeSnapshot) -> SyncAction {
    success_mark(ui);
    ui.add_space(22.0);
    ui.label(
        RichText::new("Your node is ready")
            .size(32.0)
            .color(theme::text())
            .strong(),
    );
    ui.label(
        RichText::new("Floresta is synced and checking new blocks as they arrive.")
            .size(16.0)
            .color(theme::text_muted()),
    );
    ui.add_space(26.0);
    components::card(ui, |ui| {
        ui.columns(3, |columns| {
            components::metric(
                &mut columns[0],
                format_number(snapshot.block_height),
                "Verified block",
            );
            components::metric(&mut columns[1], snapshot.peer_count.to_string(), "Peers");
            components::metric(
                &mut columns[2],
                config.wallets.len().to_string(),
                "Wallets watched",
            );
        });
    });
    ui.add_space(22.0);
    if components::primary_button(ui, "Open dashboard").clicked() {
        SyncAction::OpenDashboard
    } else {
        SyncAction::None
    }
}

fn error(ui: &mut Ui, message: &str) -> SyncAction {
    error_mark(ui);
    ui.add_space(22.0);
    ui.label(
        RichText::new("Your node needs attention")
            .size(30.0)
            .color(theme::text())
            .strong(),
    );
    ui.label(
        RichText::new("Nothing was reset. Fix the issue, then try again.")
            .size(16.0)
            .color(theme::text_muted()),
    );
    ui.add_space(22.0);
    components::inline_error(ui, message);
    ui.add_space(20.0);
    let mut action = SyncAction::None;
    ui.horizontal(|ui| {
        if components::primary_button(ui, "Try again").clicked() {
            action = SyncAction::Retry;
        }
        if components::secondary_button(ui, "View activity").clicked() {
            action = SyncAction::OpenActivity;
        }
        if components::text_button(ui, "Edit setup").clicked() {
            action = SyncAction::EditSetup;
        }
    });
    action
}

fn stopped(ui: &mut Ui) -> SyncAction {
    ui.label(
        RichText::new("Node stopped")
            .size(30.0)
            .color(theme::text())
            .strong(),
    );
    ui.label(
        RichText::new("Start it again to continue syncing and serving your wallets.")
            .size(16.0)
            .color(theme::text_muted()),
    );
    ui.add_space(22.0);
    if components::primary_button(ui, "Start node").clicked() {
        SyncAction::Retry
    } else {
        SyncAction::None
    }
}

fn sync_metric(ui: &mut Ui, title: &str, value: String, helper: &str) {
    components::card(ui, |ui| {
        ui.label(RichText::new(title).size(13.0).color(theme::text_muted()));
        ui.label(
            RichText::new(value)
                .size(23.0)
                .color(theme::text())
                .strong(),
        );
        ui.label(RichText::new(helper).size(12.0).color(theme::text_muted()));
    });
}

fn spinner(ui: &mut Ui, ctx: &egui::Context) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(72.0, 72.0), Sense::hover());
    let time = ctx.input(|input| input.time) as f32;
    let green = theme::green();
    for index in 0..10 {
        let phase = index as f32 / 10.0;
        let angle = time * 2.2 + phase * TAU;
        let center = rect.center() + egui::vec2(angle.cos(), angle.sin()) * 28.0;
        let alpha = (55.0 + phase * 190.0) as u8;
        ui.painter().circle_filled(
            center,
            3.5,
            Color32::from_rgba_unmultiplied(green.r(), green.g(), green.b(), alpha),
        );
    }
    ctx.request_repaint_after(Duration::from_millis(16));
}

fn pulse_dot(ui: &mut Ui, ctx: &egui::Context) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::hover());
    let time = ctx.input(|input| input.time) as f32;
    let radius = 4.0 + (time * 3.0).sin().abs() * 2.0;
    ui.painter()
        .circle_filled(rect.center(), radius, theme::green());
    ctx.request_repaint_after(Duration::from_millis(32));
}

fn success_mark(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(78.0, 78.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 38.0, theme::green_pale());
    ui.painter()
        .circle_stroke(rect.center(), 37.0, Stroke::new(2.0_f32, theme::green()));
    ui.painter().line_segment(
        [
            rect.center() + egui::vec2(-15.0, 1.0),
            rect.center() + egui::vec2(-4.0, 13.0),
        ],
        Stroke::new(4.0_f32, theme::green()),
    );
    ui.painter().line_segment(
        [
            rect.center() + egui::vec2(-4.0, 13.0),
            rect.center() + egui::vec2(18.0, -13.0),
        ],
        Stroke::new(4.0_f32, theme::green()),
    );
}

fn error_mark(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(72.0, 72.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 35.0, theme::red_pale());
    ui.painter()
        .circle_stroke(rect.center(), 34.0, Stroke::new(2.0_f32, theme::red()));
    ui.painter().line_segment(
        [
            rect.center() + egui::vec2(0.0, -15.0),
            rect.center() + egui::vec2(0.0, 7.0),
        ],
        Stroke::new(4.0_f32, theme::red()),
    );
    ui.painter()
        .circle_filled(rect.center() + egui::vec2(0.0, 17.0), 2.5, theme::red());
}

pub fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            output.push(',');
        }
        output.push(character);
    }
    output
}

pub fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}

pub fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_readable_metrics_have_stable_units() {
        assert_eq!(format_number(840_123), "840,123");
        assert_eq!(format_bytes(1_572_864), "1.5 MiB");
        assert_eq!(format_duration(90_061), "1d 1h");
    }

    #[test]
    fn status_surface_renders_start_sync_ready_and_error_states() {
        let ctx = egui::Context::default();
        let config = NodeConfig::default();
        let ready = NodeSnapshot {
            chain: "regtest".to_owned(),
            block_height: 100,
            target_height: 100,
            verification_progress: 1.0,
            initial_block_download: false,
            peer_count: 2,
            inbound_peers: 0,
            outbound_peers: 2,
            v2_peers: 2,
            peers: Vec::new(),
            uptime_seconds: 60,
            size_on_disk: 1024,
            last_block_time: 0,
            user_agent: "/Floresta:test/".to_owned(),
            received_at: std::time::Instant::now(),
        };
        let mut syncing = ready.clone();
        syncing.block_height = 42;
        syncing.initial_block_download = true;
        syncing.verification_progress = 0.42;

        for initial_dark_mode in [false, true] {
            theme::apply(&ctx, initial_dark_mode);
            let mut dark_mode = initial_dark_mode;
            render_status(
                &ctx,
                &config,
                SyncStatus::Starting("Opening database"),
                &mut dark_mode,
            );
            render_status(&ctx, &config, SyncStatus::Syncing(&syncing), &mut dark_mode);
            render_status(&ctx, &config, SyncStatus::Ready(&ready), &mut dark_mode);
            render_status(
                &ctx,
                &config,
                SyncStatus::Error("Port already in use"),
                &mut dark_mode,
            );
            render_status(&ctx, &config, SyncStatus::Stopped, &mut dark_mode);
            assert_eq!(dark_mode, initial_dark_mode);
        }
    }

    fn render_status(
        ctx: &egui::Context,
        config: &NodeConfig,
        status: SyncStatus<'_>,
        dark_mode: &mut bool,
    ) {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1180.0, 780.0),
                )),
                ..Default::default()
            },
            |ctx| {
                let _ = show(ctx, config, status, dark_mode);
            },
        );
        assert!(!output.shapes.is_empty());
    }
}
