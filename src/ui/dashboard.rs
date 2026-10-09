// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use eframe::egui;
use egui::Align;
use egui::Color32;
use egui::CornerRadius;
use egui::Layout;
use egui::RichText;
use egui::Stroke;
use egui::Ui;

use super::components;
use super::sync::format_bytes;
use super::sync::format_duration;
use super::sync::format_number;
use super::theme;
use crate::config::NodeConfig;
use crate::config::WalletKind;
use crate::node::NodeSnapshot;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DashboardView {
    #[default]
    Overview,
    Sync,
    Wallets,
    Peers,
    Services,
    Activity,
    Settings,
}

impl DashboardView {
    const ALL: [Self; 7] = [
        Self::Overview,
        Self::Sync,
        Self::Wallets,
        Self::Peers,
        Self::Services,
        Self::Activity,
        Self::Settings,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Sync => "Sync",
            Self::Wallets => "Wallets",
            Self::Peers => "Peers",
            Self::Services => "Services",
            Self::Activity => "Activity",
            Self::Settings => "Settings",
        }
    }

    const fn subtitle(self) -> &'static str {
        match self {
            Self::Overview => "The essentials, at a glance",
            Self::Sync => "Chain verification and progress",
            Self::Wallets => "Watch-only wallet coverage",
            Self::Peers => "Your connections to Bitcoin",
            Self::Services => "Connections for wallets and apps",
            Self::Activity => "Important events from this session",
            Self::Settings => "Your current node configuration",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DashboardAction {
    None,
    Restart,
    Stop,
    EditSetup,
    OpenDataFolder,
    SetDarkMode(bool),
}

pub fn show(
    ctx: &egui::Context,
    view: &mut DashboardView,
    snapshot: &NodeSnapshot,
    config: &NodeConfig,
    dark_mode: bool,
    activity: &[String],
    last_error: Option<&str>,
) -> DashboardAction {
    let compact = ctx.screen_rect().width() < 820.0;
    if compact {
        mobile_navigation(ctx, view, snapshot);
    } else {
        sidebar(ctx, view, snapshot, config);
    }

    let mut action = DashboardAction::None;
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::background())
                .inner_margin(egui::Margin::symmetric(28, 22)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(1120.0);
                    page_header(ui, *view, snapshot);
                    ui.add_space(22.0);
                    action = match *view {
                        DashboardView::Overview => overview(ui, view, snapshot, config, last_error),
                        DashboardView::Sync => sync_page(ui, snapshot, config),
                        DashboardView::Wallets => wallets_page(ui, config),
                        DashboardView::Peers => peers_page(ui, snapshot),
                        DashboardView::Services => services_page(ui, config),
                        DashboardView::Activity => activity_page(ui, activity, last_error),
                        DashboardView::Settings => settings_page(ui, config, snapshot, dark_mode),
                    };
                });
        });
    action
}

fn sidebar(
    ctx: &egui::Context,
    view: &mut DashboardView,
    snapshot: &NodeSnapshot,
    config: &NodeConfig,
) {
    egui::SidePanel::left("desktop-navigation")
        .exact_width(220.0)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(theme::surface())
                .stroke(Stroke::new(1.0_f32, theme::border()))
                .inner_margin(egui::Margin::symmetric(16, 20)),
        )
        .show(ctx, |ui| {
            components::brand(ui, true);
            ui.add_space(26.0);
            for destination in DashboardView::ALL {
                navigation_button(ui, view, destination, 188.0);
                ui.add_space(2.0);
            }

            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                ui.label(
                    RichText::new(format!("Desktop {}", env!("CARGO_PKG_VERSION")))
                        .size(11.0)
                        .color(theme::text_muted()),
                );
                ui.add_space(10.0);
                components::muted_card(ui, |ui| {
                    ui.label(
                        RichText::new(if snapshot.is_ready() {
                            "Node ready"
                        } else {
                            "Node syncing"
                        })
                        .color(if snapshot.is_ready() {
                            theme::green_dark()
                        } else {
                            theme::amber()
                        })
                        .strong(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} · {} peers",
                            config.network.title(),
                            snapshot.peer_count
                        ))
                        .size(12.0)
                        .color(theme::text_muted()),
                    );
                });
            });
        });
}

fn mobile_navigation(ctx: &egui::Context, view: &mut DashboardView, snapshot: &NodeSnapshot) {
    egui::TopBottomPanel::top("mobile-navigation")
        .frame(
            egui::Frame::new()
                .fill(theme::surface())
                .stroke(Stroke::new(1.0_f32, theme::border()))
                .inner_margin(egui::Margin::symmetric(16, 10)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                components::brand(ui, true);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    components::status_pill(
                        ui,
                        if snapshot.is_ready() {
                            "READY"
                        } else {
                            "SYNCING"
                        },
                        if snapshot.is_ready() {
                            theme::green_dark()
                        } else {
                            theme::amber()
                        },
                        if snapshot.is_ready() {
                            theme::green_pale()
                        } else {
                            theme::amber_pale()
                        },
                    );
                });
            });
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for destination in DashboardView::ALL {
                        navigation_button(ui, view, destination, 98.0);
                    }
                });
            });
        });
}

fn navigation_button(
    ui: &mut Ui,
    current: &mut DashboardView,
    destination: DashboardView,
    width: f32,
) {
    let selected = *current == destination;
    let response = ui.add_sized(
        [width, 38.0],
        egui::Button::new(
            RichText::new(destination.label())
                .color(if selected {
                    theme::green_dark()
                } else {
                    theme::text_muted()
                })
                .strong(),
        )
        .fill(if selected {
            theme::green_pale()
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(7)),
    );
    if response.clicked() {
        *current = destination;
    }
}

fn page_header(ui: &mut Ui, view: DashboardView, snapshot: &NodeSnapshot) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| components::page_title(ui, view.label(), view.subtitle()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (label, color, background) = if snapshot.is_ready() {
                ("READY", theme::green_dark(), theme::green_pale())
            } else {
                ("SYNCING", theme::amber(), theme::amber_pale())
            };
            components::status_pill(ui, label, color, background);
        });
    });
}

fn overview(
    ui: &mut Ui,
    view: &mut DashboardView,
    snapshot: &NodeSnapshot,
    config: &NodeConfig,
    last_error: Option<&str>,
) -> DashboardAction {
    if let Some(error) = last_error {
        status_banner(
            ui,
            "Status was interrupted",
            error,
            theme::red(),
            theme::red_pale(),
            "Open activity",
            DashboardView::Activity,
            view,
        );
    } else if snapshot.is_ready() {
        status_banner(
            ui,
            "Your node is ready",
            "Floresta is synced, connected and checking new blocks.",
            theme::green_dark(),
            theme::green_pale(),
            "View sync",
            DashboardView::Sync,
            view,
        );
    } else {
        status_banner(
            ui,
            "Your node is catching up",
            "Wallet history may be incomplete until verification finishes.",
            theme::amber(),
            theme::amber_pale(),
            "View sync",
            DashboardView::Sync,
            view,
        );
    }

    ui.add_space(18.0);
    if ui.available_width() >= 840.0 {
        ui.columns(3, |columns| {
            network_overview_card(&mut columns[0], snapshot, view);
            wallet_overview_card(&mut columns[1], config, view);
            system_overview_card(&mut columns[2], snapshot);
        });
    } else {
        network_overview_card(ui, snapshot, view);
        ui.add_space(12.0);
        wallet_overview_card(ui, config, view);
        ui.add_space(12.0);
        system_overview_card(ui, snapshot);
    }

    ui.add_space(18.0);
    components::card(ui, |ui| {
        ui.horizontal(|ui| {
            components::card_title(ui, "Chain verification");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.1}%", snapshot.verification_progress * 100.0))
                        .color(theme::text())
                        .strong(),
                );
            });
        });
        ui.add_space(9.0);
        components::progress_bar(ui, snapshot.verification_progress);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "Block {} of {}",
                    format_number(snapshot.block_height),
                    format_number(snapshot.target_height)
                ))
                .color(theme::text_muted()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if components::text_button(ui, "View sync").clicked() {
                    *view = DashboardView::Sync;
                }
            });
        });
    });

    ui.add_space(18.0);
    if ui.available_width() >= 700.0 {
        ui.columns(2, |columns| {
            peers_summary(&mut columns[0], snapshot, view);
            services_summary(&mut columns[1], config, view);
        });
    } else {
        peers_summary(ui, snapshot, view);
        ui.add_space(12.0);
        services_summary(ui, config, view);
    }

    ui.add_space(18.0);
    components::muted_card(ui, |ui| {
        components::card_title(ui, "Why this matters");
        ui.label(
            RichText::new(
                "Your Floresta node checks Bitcoin's rules itself instead of trusting a website or wallet provider.",
            )
            .color(theme::text_muted()),
        );
    });

    DashboardAction::None
}

#[allow(clippy::too_many_arguments)]
fn status_banner(
    ui: &mut Ui,
    title: &str,
    detail: &str,
    color: Color32,
    background: Color32,
    action_label: &str,
    destination: DashboardView,
    view: &mut DashboardView,
) {
    egui::Frame::new()
        .fill(background)
        .stroke(Stroke::new(1.0_f32, color.linear_multiply(0.25)))
        .corner_radius(CornerRadius::same(11))
        .inner_margin(egui::Margin::same(18))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(18.0).color(color).strong());
                    ui.label(RichText::new(detail).color(theme::text()));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if components::text_button(ui, action_label).clicked() {
                        *view = destination;
                    }
                });
            });
        });
}

fn network_overview_card(ui: &mut Ui, snapshot: &NodeSnapshot, view: &mut DashboardView) {
    components::card(ui, |ui| {
        components::card_title(ui, "Network");
        ui.add_space(10.0);
        components::metric(ui, snapshot.peer_count.to_string(), "connected peers");
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!(
                "{} outbound · {} using v2",
                snapshot.outbound_peers, snapshot.v2_peers
            ))
            .size(13.0)
            .color(theme::text_muted()),
        );
        if components::text_button(ui, "View peers").clicked() {
            *view = DashboardView::Peers;
        }
    });
}

fn wallet_overview_card(ui: &mut Ui, config: &NodeConfig, view: &mut DashboardView) {
    components::card(ui, |ui| {
        components::card_title(ui, "Wallets");
        ui.add_space(10.0);
        components::metric(ui, config.wallets.len().to_string(), "wallet definitions");
        ui.add_space(8.0);
        ui.label(
            RichText::new("Watch-only data is indexed locally")
                .size(13.0)
                .color(theme::text_muted()),
        );
        if components::text_button(ui, "View wallets").clicked() {
            *view = DashboardView::Wallets;
        }
    });
}

fn system_overview_card(ui: &mut Ui, snapshot: &NodeSnapshot) {
    components::card(ui, |ui| {
        components::card_title(ui, "System");
        ui.add_space(10.0);
        components::metric(ui, format_duration(snapshot.uptime_seconds), "node uptime");
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!(
                "{} chain state on disk",
                format_bytes(snapshot.size_on_disk)
            ))
            .size(13.0)
            .color(theme::text_muted()),
        );
        ui.label(
            RichText::new(if snapshot.received_at.elapsed().as_secs() < 8 {
                "Status updated just now"
            } else {
                "Status update delayed"
            })
            .size(13.0)
            .color(theme::text_muted()),
        );
    });
}

fn peers_summary(ui: &mut Ui, snapshot: &NodeSnapshot, view: &mut DashboardView) {
    components::card(ui, |ui| {
        components::card_title(ui, "Peer connections");
        ui.add_space(8.0);
        if snapshot.peer_count == 0 {
            ui.label(RichText::new("No peers yet").color(theme::amber()).strong());
            ui.label(
                RichText::new("Floresta is still looking for Bitcoin peers.")
                    .color(theme::text_muted()),
            );
        } else {
            ui.label(
                RichText::new(format!(
                    "{} connected · {} inbound · {} outbound",
                    snapshot.peer_count, snapshot.inbound_peers, snapshot.outbound_peers
                ))
                .color(theme::text()),
            );
        }
        if components::text_button(ui, "View peers").clicked() {
            *view = DashboardView::Peers;
        }
    });
}

fn services_summary(ui: &mut Ui, config: &NodeConfig, view: &mut DashboardView) {
    components::card(ui, |ui| {
        components::card_title(ui, "Services");
        ui.add_space(8.0);
        service_line(ui, "Electrum", true);
        service_line(ui, "Wallet index", true);
        service_line(ui, "JSON-RPC", config.rpc_enabled);
        if components::text_button(ui, "View services").clicked() {
            *view = DashboardView::Services;
        }
    });
}

fn sync_page(ui: &mut Ui, snapshot: &NodeSnapshot, config: &NodeConfig) -> DashboardAction {
    components::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                components::card_title(ui, snapshot.phase());
                ui.label(
                    RichText::new(
                        "Floresta verifies every accepted block against Bitcoin's rules.",
                    )
                    .color(theme::text_muted()),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.1}%", snapshot.verification_progress * 100.0))
                        .size(26.0)
                        .color(theme::text())
                        .strong(),
                );
            });
        });
        ui.add_space(18.0);
        components::progress_bar(ui, snapshot.verification_progress);
        ui.add_space(14.0);
        ui.columns(3, |columns| {
            components::metric(
                &mut columns[0],
                format_number(snapshot.block_height),
                "verified height",
            );
            components::metric(
                &mut columns[1],
                format_number(snapshot.target_height),
                "header height",
            );
            components::metric(
                &mut columns[2],
                format_number(snapshot.blocks_behind()),
                "blocks behind",
            );
        });
    });

    ui.add_space(16.0);
    components::card(ui, |ui| {
        components::card_title(ui, "Sync details");
        ui.add_space(8.0);
        detail_row(ui, "Phase", snapshot.phase());
        detail_row(ui, "Network", &snapshot.chain);
        detail_row(ui, "Latest block", &format_number(snapshot.block_height));
        detail_row(ui, "Last block seen", &format_age(snapshot.last_block_time));
        detail_row(ui, "Chain state size", &format_bytes(snapshot.size_on_disk));
    });

    if config.fast_start && config.backfill {
        ui.add_space(16.0);
        components::card(ui, |ui| {
            ui.horizontal(|ui| {
                components::card_title(ui, "Background validation");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    components::status_pill(ui, "ON", theme::green_dark(), theme::green_pale());
                });
            });
            ui.label(
                RichText::new(
                    "Validating blocks skipped by fast start. It does not prevent the node from serving wallets. Exact progress is not exposed by this node version.",
                )
                .color(theme::text_muted()),
            );
        });
    }

    DashboardAction::None
}

fn wallets_page(ui: &mut Ui, config: &NodeConfig) -> DashboardAction {
    let mut action = DashboardAction::None;
    components::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                components::card_title(ui, "Watch-only index");
                ui.label(
                    RichText::new("Public wallet data is indexed locally.")
                        .color(theme::text_muted()),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                components::status_pill(ui, "ON", theme::green_dark(), theme::green_pale());
            });
        });
        ui.add_space(16.0);
        components::metric(ui, config.wallets.len().to_string(), "wallet definitions");
        ui.add_space(10.0);
        ui.label(
            RichText::new(
                "Live balance and transaction totals are not exposed by this Floresta version.",
            )
            .size(13.0)
            .color(theme::text_muted()),
        );
    });

    ui.add_space(16.0);
    if config.wallets.is_empty() {
        components::muted_card(ui, |ui| {
            components::card_title(ui, "No wallets added");
            ui.label(
                RichText::new(
                    "Add a descriptor or extended public key to watch wallet activity through your own node.",
                )
                .color(theme::text_muted()),
            );
            if components::primary_button(ui, "Add a wallet").clicked() {
                action = DashboardAction::EditSetup;
            }
        });
    } else {
        for wallet in &config.wallets {
            components::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        components::card_title(
                            ui,
                            if wallet.label.is_empty() {
                                wallet.kind.title()
                            } else {
                                &wallet.label
                            },
                        );
                        ui.label(
                            RichText::new(match wallet.kind {
                                WalletKind::Descriptor => "Output descriptor",
                                WalletKind::ExtendedPublicKey => "Extended public key",
                            })
                            .color(theme::text_muted()),
                        );
                        ui.label(
                            RichText::new(shorten(&wallet.value, 72))
                                .monospace()
                                .size(12.0)
                                .color(theme::text_muted()),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        components::status_pill(
                            ui,
                            "WATCHING",
                            theme::green_dark(),
                            theme::green_pale(),
                        );
                    });
                });
            });
            ui.add_space(10.0);
        }
    }

    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "Balances remain in your wallet application. Connect it through Electrum to use Floresta's verified view of Bitcoin.",
        )
        .size(13.0)
        .color(theme::text_muted()),
    );
    action
}

fn peers_page(ui: &mut Ui, snapshot: &NodeSnapshot) -> DashboardAction {
    components::card(ui, |ui| {
        ui.columns(3, |columns| {
            components::metric(
                &mut columns[0],
                snapshot.peer_count.to_string(),
                "total peers",
            );
            components::metric(
                &mut columns[1],
                snapshot.outbound_peers.to_string(),
                "outbound",
            );
            components::metric(
                &mut columns[2],
                snapshot.v2_peers.to_string(),
                "v2 transport",
            );
        });
    });
    ui.add_space(16.0);

    if snapshot.peers.is_empty() {
        components::muted_card(ui, |ui| {
            components::card_title(ui, "Waiting for peers");
            ui.label(
                RichText::new(
                    "Floresta is discovering peers. Connections appear here automatically.",
                )
                .color(theme::text_muted()),
            );
        });
    } else {
        for peer in &snapshot.peers {
            components::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(&peer.address)
                                .monospace()
                                .color(theme::text())
                                .strong(),
                        );
                        ui.label(
                            RichText::new(&peer.user_agent)
                                .size(12.0)
                                .color(theme::text_muted()),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        components::status_pill(
                            ui,
                            &peer.transport,
                            theme::blue(),
                            theme::blue_pale(),
                        );
                        ui.label(
                            RichText::new(if peer.inbound { "Inbound" } else { "Outbound" })
                                .size(12.0)
                                .color(theme::text_muted()),
                        );
                        ui.label(
                            RichText::new(&peer.state)
                                .size(12.0)
                                .color(theme::text_muted()),
                        );
                    });
                });
            });
            ui.add_space(9.0);
        }
    }
    DashboardAction::None
}

fn services_page(ui: &mut Ui, config: &NodeConfig) -> DashboardAction {
    let mut action = DashboardAction::None;
    service_card(
        ui,
        "Electrum server",
        "Connect Sparrow, Electrum and compatible wallets.",
        true,
        Some(&config.electrum_address),
    );
    ui.add_space(12.0);
    service_card(
        ui,
        "Wallet index",
        "Indexes public wallet activity for local queries.",
        true,
        None,
    );
    ui.add_space(12.0);
    service_card(
        ui,
        "JSON-RPC",
        "Developer interface for compatible apps and command-line tools.",
        config.rpc_enabled,
        config.rpc_enabled.then_some(config.rpc_address.as_str()),
    );
    if config.electrum_tls_enabled {
        ui.add_space(12.0);
        service_card(
            ui,
            "Electrum TLS",
            "Encrypted Electrum connections using a self-signed certificate.",
            true,
            Some(&config.electrum_tls_address),
        );
    }
    ui.add_space(18.0);
    components::muted_card(ui, |ui| {
        ui.label(
            RichText::new("Advanced services are off by default")
                .color(theme::text())
                .strong(),
        );
        ui.label(
            RichText::new(
                "Only enable an endpoint when another application needs it. Loopback addresses keep access on this computer.",
            )
            .color(theme::text_muted()),
        );
        if components::secondary_button(ui, "Change services").clicked() {
            action = DashboardAction::EditSetup;
        }
    });
    action
}

fn service_card(
    ui: &mut Ui,
    title: &str,
    description: &str,
    enabled: bool,
    endpoint: Option<&str>,
) {
    components::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                components::card_title(ui, title);
                ui.label(RichText::new(description).color(theme::text_muted()));
                if let Some(endpoint) = endpoint {
                    ui.label(
                        RichText::new(endpoint)
                            .monospace()
                            .size(13.0)
                            .color(theme::text()),
                    );
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                components::status_pill(
                    ui,
                    if enabled { "READY" } else { "OFF" },
                    if enabled {
                        theme::green_dark()
                    } else {
                        theme::text_muted()
                    },
                    if enabled {
                        theme::green_pale()
                    } else {
                        theme::surface_muted()
                    },
                );
            });
        });
    });
}

fn activity_page(ui: &mut Ui, activity: &[String], last_error: Option<&str>) -> DashboardAction {
    if let Some(error) = last_error {
        components::inline_error(ui, error);
        ui.add_space(14.0);
    }
    components::card(ui, |ui| {
        if activity.is_empty() {
            ui.label(RichText::new("No activity yet").color(theme::text_muted()));
            return;
        }
        for (index, entry) in activity.iter().rev().enumerate() {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::vec2(14.0, 20.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(dot.center(), 3.0, theme::green());
                ui.label(RichText::new(entry).color(theme::text()));
            });
            if index + 1 != activity.len() {
                components::divider(ui);
                ui.add_space(6.0);
            }
        }
    });
    DashboardAction::None
}

fn settings_page(
    ui: &mut Ui,
    config: &NodeConfig,
    snapshot: &NodeSnapshot,
    dark_mode: bool,
) -> DashboardAction {
    let mut action = DashboardAction::None;
    components::card(ui, |ui| {
        components::card_title(ui, "Appearance");
        ui.add_space(10.0);
        let mut enabled = dark_mode;
        if components::toggle_setting(
            ui,
            &mut enabled,
            "Dark mode",
            "Use a low-glare palette across setup, sync and the dashboard.",
        )
        .changed()
        {
            action = DashboardAction::SetDarkMode(enabled);
        }
    });

    ui.add_space(16.0);
    components::card(ui, |ui| {
        components::card_title(ui, "Node essentials");
        ui.add_space(10.0);
        detail_row(ui, "Network", config.network.title());
        detail_row(ui, "Data folder", &config.data_dir);
        detail_row(ui, "Privacy", if config.use_tor { "Tor" } else { "Direct" });
        detail_row(ui, "Wallets", &config.wallet_summary());
        detail_row(ui, "Node version", &snapshot.user_agent);
        ui.add_space(12.0);
        if components::primary_button(ui, "Edit setup").clicked() {
            action = DashboardAction::EditSetup;
        }
        if components::secondary_button(ui, "Open data folder").clicked() {
            action = DashboardAction::OpenDataFolder;
        }
    });

    ui.add_space(16.0);
    egui::CollapsingHeader::new("Advanced node controls")
        .default_open(false)
        .show(ui, |ui| {
            components::muted_card(ui, |ui| {
                ui.label(
                    RichText::new("Restart or stop the embedded node process.")
                        .color(theme::text_muted()),
                );
                ui.horizontal(|ui| {
                    if components::secondary_button(ui, "Restart node").clicked() {
                        action = DashboardAction::Restart;
                    }
                    if components::text_button(ui, "Stop node").clicked() {
                        action = DashboardAction::Stop;
                    }
                });
            });
        });
    action
}

fn service_line(ui: &mut Ui, title: &str, enabled: bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).color(theme::text()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(if enabled { "Ready" } else { "Off" })
                    .size(12.0)
                    .color(if enabled {
                        theme::green_dark()
                    } else {
                        theme::text_muted()
                    })
                    .strong(),
            );
        });
    });
}

fn detail_row(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::text_muted()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(shorten(value, 64))
                    .color(theme::text())
                    .strong(),
            );
        });
    });
    ui.add_space(8.0);
    components::divider(ui);
    ui.add_space(8.0);
}

fn format_age(timestamp: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(timestamp, |duration| duration.as_secs());
    let age = now.saturating_sub(timestamp);
    if age < 60 {
        "just now".to_owned()
    } else if age < 3_600 {
        format!("{} minutes ago", age / 60)
    } else if age < 86_400 {
        format!("{} hours ago", age / 3_600)
    } else {
        format!("{} days ago", age / 86_400)
    }
}

fn shorten(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_owned();
    }
    let prefix: String = value.chars().take(max_chars.saturating_sub(9)).collect();
    let suffix: String = value.chars().skip(count.saturating_sub(6)).collect();
    format!("{prefix}…{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_block_timestamp_is_described_without_calendar_assumptions() {
        assert!(format_age(0).ends_with("days ago"));
    }

    #[test]
    fn responsive_dashboard_renders_every_destination() {
        let ctx = egui::Context::default();
        let config = NodeConfig {
            wallets: vec![crate::config::WalletImport {
                label: "Savings".to_owned(),
                kind: WalletKind::Descriptor,
                value: "wpkh(xpub-example/0/*)".to_owned(),
            }],
            ..NodeConfig::default()
        };
        let snapshot = NodeSnapshot {
            chain: "regtest".to_owned(),
            block_height: 100,
            target_height: 100,
            verification_progress: 1.0,
            initial_block_download: false,
            peer_count: 1,
            inbound_peers: 0,
            outbound_peers: 1,
            v2_peers: 1,
            peers: vec![crate::node::PeerSnapshot {
                address: "127.0.0.1:18444".to_owned(),
                user_agent: "/Floresta:test/".to_owned(),
                state: "Ready".to_owned(),
                transport: "V2".to_owned(),
                inbound: false,
            }],
            uptime_seconds: 3_661,
            size_on_disk: 1_048_576,
            last_block_time: 0,
            user_agent: "/Floresta:test/".to_owned(),
            received_at: std::time::Instant::now(),
        };

        for dark_mode in [false, true] {
            theme::apply(&ctx, dark_mode);
            for width in [1180.0, 700.0] {
                for destination in DashboardView::ALL {
                    let mut current = destination;
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 780.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            let _ = show(
                                ctx,
                                &mut current,
                                &snapshot,
                                &config,
                                dark_mode,
                                &["00:00  Node started".to_owned()],
                                None,
                            );
                        },
                    );
                    assert!(
                        !output.shapes.is_empty(),
                        "{} rendered no shapes at {width}px in dark_mode={dark_mode}",
                        destination.label()
                    );
                }
            }
        }
    }
}
