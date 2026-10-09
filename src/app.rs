// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::Path;
use std::process::Command;
use std::time::Duration;
use std::time::Instant;

use eframe::egui;
use egui::Align;
use egui::Layout;
use egui::RichText;
use serde::Deserialize;
use serde::Serialize;

use crate::config::NodeConfig;
use crate::node::NodeController;
use crate::node::NodeEvent;
use crate::node::NodeSnapshot;
use crate::ui::components;
use crate::ui::dashboard;
use crate::ui::dashboard::DashboardAction;
use crate::ui::dashboard::DashboardView;
use crate::ui::setup;
use crate::ui::setup::SetupState;
use crate::ui::sync;
use crate::ui::sync::SyncAction;
use crate::ui::sync::SyncStatus;
use crate::ui::theme;

const STORAGE_KEY: &str = "floresta-desktop-state";

#[derive(Default, Deserialize, Serialize)]
struct PersistedState {
    config: Option<NodeConfig>,
    dark_mode: Option<bool>,
}

enum Lifecycle {
    Setup,
    Starting(String),
    Running,
    Error(String),
    Stopped,
}

pub struct FlorestaApp {
    config: Option<NodeConfig>,
    setup: SetupState,
    lifecycle: Lifecycle,
    controller: NodeController,
    snapshot: Option<NodeSnapshot>,
    dashboard_view: DashboardView,
    ready_acknowledged: bool,
    show_error_activity: bool,
    activity: Vec<String>,
    last_error: Option<String>,
    dark_mode: bool,
    session_started: Instant,
}

impl FlorestaApp {
    pub fn new(creation: &eframe::CreationContext<'_>) -> Self {
        let persisted = creation
            .storage
            .and_then(|storage| eframe::get_value::<PersistedState>(storage, STORAGE_KEY))
            .unwrap_or_default();
        let dark_mode = persisted
            .dark_mode
            .unwrap_or_else(|| creation.egui_ctx.system_theme() == Some(egui::Theme::Dark));
        theme::apply(&creation.egui_ctx, dark_mode);
        let controller = NodeController::new();
        let setup = SetupState::new(persisted.config.clone().unwrap_or_default());
        let mut app = Self {
            config: persisted.config,
            setup,
            lifecycle: Lifecycle::Setup,
            controller,
            snapshot: None,
            dashboard_view: DashboardView::Overview,
            ready_acknowledged: false,
            show_error_activity: false,
            activity: Vec::new(),
            last_error: None,
            dark_mode,
            session_started: Instant::now(),
        };

        if app.config.is_some() {
            app.start_node();
        }
        app
    }

    fn start_node(&mut self) {
        let Some(config) = self.config.clone() else {
            self.lifecycle = Lifecycle::Setup;
            return;
        };

        self.snapshot = None;
        self.last_error = None;
        self.ready_acknowledged = false;
        self.show_error_activity = false;
        self.dashboard_view = DashboardView::Overview;
        self.lifecycle = Lifecycle::Starting("Preparing node settings".to_owned());
        self.push_activity(format!("Starting node on {}", config.network.title()));
        if let Err(error) = self.controller.start(config) {
            self.record_error(error);
        }
    }

    fn restart_node(&mut self) {
        self.controller.stop();
        self.push_activity("Restart requested".to_owned());
        self.start_node();
    }

    fn edit_setup(&mut self) {
        self.controller.stop();
        let draft = self.config.clone().unwrap_or_default();
        self.setup = SetupState::new(draft);
        self.lifecycle = Lifecycle::Setup;
        self.snapshot = None;
        self.ready_acknowledged = false;
        self.show_error_activity = false;
    }

    fn handle_event(&mut self, event: NodeEvent) {
        match event {
            NodeEvent::Starting(detail) => {
                if !matches!(self.lifecycle, Lifecycle::Setup) {
                    self.lifecycle = Lifecycle::Starting(detail);
                }
            }
            NodeEvent::Snapshot(snapshot) => {
                let first_snapshot = self.snapshot.is_none();
                self.snapshot = Some(snapshot);
                self.lifecycle = Lifecycle::Running;
                self.last_error = None;
                if first_snapshot {
                    self.push_activity("Live node status connected".to_owned());
                }
            }
            NodeEvent::Activity(message) => self.push_activity(message),
            NodeEvent::Error(error) => self.record_error(error),
            NodeEvent::Stopped => {
                if !matches!(self.lifecycle, Lifecycle::Setup) {
                    self.lifecycle = Lifecycle::Stopped;
                    self.push_activity("Node stopped safely".to_owned());
                }
            }
        }
    }

    fn record_error(&mut self, error: String) {
        self.last_error = Some(error.clone());
        self.lifecycle = Lifecycle::Error(error.clone());
        self.push_activity(format!("Attention needed: {error}"));
    }

    fn push_activity(&mut self, message: String) {
        let elapsed = self.session_started.elapsed().as_secs();
        let entry = format!("{:02}:{:02}  {message}", elapsed / 60, elapsed % 60);
        self.activity.push(entry);
        if self.activity.len() > 100 {
            self.activity.remove(0);
        }
    }

    fn handle_sync_action(&mut self, action: SyncAction) {
        match action {
            SyncAction::None => {}
            SyncAction::OpenDashboard => {
                self.ready_acknowledged = true;
                self.dashboard_view = DashboardView::Overview;
            }
            SyncAction::OpenActivity => {
                self.show_error_activity = true;
                self.ready_acknowledged = true;
                self.dashboard_view = DashboardView::Activity;
            }
            SyncAction::Retry => self.restart_node(),
            SyncAction::EditSetup => self.edit_setup(),
        }
    }

    fn handle_dashboard_action(&mut self, ctx: &egui::Context, action: DashboardAction) {
        match action {
            DashboardAction::None => {}
            DashboardAction::Restart => self.restart_node(),
            DashboardAction::Stop => {
                self.controller.stop();
                self.lifecycle = Lifecycle::Stopped;
                self.push_activity("Stop requested".to_owned());
            }
            DashboardAction::EditSetup => self.edit_setup(),
            DashboardAction::OpenDataFolder => {
                if let Some(config) = &self.config {
                    let path = config.node_data_dir();
                    match open_path(&path) {
                        Ok(()) => self.push_activity("Opened the node data folder".to_owned()),
                        Err(error) => self.push_activity(error),
                    }
                }
            }
            DashboardAction::SetDarkMode(dark_mode) => {
                self.dark_mode = dark_mode;
                theme::apply(ctx, dark_mode);
                ctx.request_repaint();
            }
        }
    }

    fn show_activity_without_snapshot(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::background())
                    .inner_margin(egui::Margin::symmetric(24, 20)),
            )
            .show(ctx, |ui| {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.set_max_width(800.0);
                    components::brand(ui, true);
                    ui.add_space(28.0);
                    components::page_title(ui, "Activity", "Important events from this session");
                    ui.add_space(18.0);
                    components::card(ui, |ui| {
                        for entry in self.activity.iter().rev() {
                            ui.label(RichText::new(entry).color(theme::text()));
                            components::divider(ui);
                            ui.add_space(5.0);
                        }
                    });
                    ui.add_space(18.0);
                    if components::primary_button(ui, "Back to status").clicked() {
                        self.show_error_activity = false;
                    }
                });
            });
    }
}

impl eframe::App for FlorestaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Some(event) = self.controller.try_recv() {
            self.handle_event(event);
        }

        match &self.lifecycle {
            Lifecycle::Setup => {
                if let Some(config) = setup::show(ctx, &mut self.setup, &mut self.dark_mode) {
                    self.config = Some(config);
                    self.start_node();
                }
            }
            Lifecycle::Starting(detail) => {
                let Some(config) = self.config.as_ref() else {
                    self.edit_setup();
                    return;
                };
                let action = sync::show(
                    ctx,
                    config,
                    SyncStatus::Starting(detail),
                    &mut self.dark_mode,
                );
                self.handle_sync_action(action);
            }
            Lifecycle::Running => {
                let (Some(config), Some(snapshot)) = (self.config.as_ref(), self.snapshot.as_ref())
                else {
                    self.lifecycle = Lifecycle::Starting("Waiting for node status".to_owned());
                    return;
                };

                if !self.ready_acknowledged {
                    let status = if snapshot.is_ready() {
                        SyncStatus::Ready(snapshot)
                    } else {
                        SyncStatus::Syncing(snapshot)
                    };
                    let action = sync::show(ctx, config, status, &mut self.dark_mode);
                    self.handle_sync_action(action);
                } else {
                    let action = dashboard::show(
                        ctx,
                        &mut self.dashboard_view,
                        snapshot,
                        config,
                        self.dark_mode,
                        &self.activity,
                        self.last_error.as_deref(),
                    );
                    self.handle_dashboard_action(ctx, action);
                }
            }
            Lifecycle::Error(error) => {
                if self.show_error_activity {
                    if let (Some(config), Some(snapshot)) =
                        (self.config.as_ref(), self.snapshot.as_ref())
                    {
                        let action = dashboard::show(
                            ctx,
                            &mut self.dashboard_view,
                            snapshot,
                            config,
                            self.dark_mode,
                            &self.activity,
                            self.last_error.as_deref(),
                        );
                        self.handle_dashboard_action(ctx, action);
                    } else {
                        self.show_activity_without_snapshot(ctx);
                    }
                } else if let Some(config) = self.config.as_ref() {
                    let action =
                        sync::show(ctx, config, SyncStatus::Error(error), &mut self.dark_mode);
                    self.handle_sync_action(action);
                }
            }
            Lifecycle::Stopped => {
                if let Some(config) = self.config.as_ref() {
                    let action = sync::show(ctx, config, SyncStatus::Stopped, &mut self.dark_mode);
                    self.handle_sync_action(action);
                } else {
                    self.edit_setup();
                }
            }
        }

        if !matches!(self.lifecycle, Lifecycle::Setup) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            STORAGE_KEY,
            &PersistedState {
                config: self.config.clone(),
                dark_mode: Some(self.dark_mode),
            },
        );
    }
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");

    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open the data folder: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_state_without_theme_migrates_without_losing_node_config() {
        let previous_state = r#"{"config":null}"#;
        let persisted: PersistedState = serde_json::from_str(previous_state).unwrap();
        assert!(persisted.config.is_none());
        assert!(persisted.dark_mode.is_none());
    }
}
