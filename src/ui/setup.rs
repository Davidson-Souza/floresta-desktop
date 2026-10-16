// SPDX-License-Identifier: MIT OR Apache-2.0

use eframe::egui;
use egui::Align;
use egui::Color32;
use egui::CornerRadius;
use egui::Layout;
use egui::RichText;
use egui::Sense;
use egui::Stroke;
use egui::TextEdit;
use egui::Ui;

use super::components;
use super::theme;
use crate::config::NodeConfig;
use crate::config::NodeNetwork;
use crate::config::WalletImport;
use crate::config::WalletKind;
use crate::config::validate_wallet;

const STEPS: [&str; 5] = ["Network", "Storage", "Wallets", "Services", "Review"];

pub struct SetupState {
    pub draft: NodeConfig,
    step: usize,
    furthest_step: usize,
    error: Option<String>,
    show_network_advanced: bool,
    show_sync_advanced: bool,
    show_service_advanced: bool,
    wallet_kind: WalletKind,
    wallet_label: String,
    wallet_value: String,
}

impl SetupState {
    pub fn new(draft: NodeConfig) -> Self {
        Self {
            draft,
            step: 0,
            furthest_step: 0,
            error: None,
            show_network_advanced: false,
            show_sync_advanced: false,
            show_service_advanced: false,
            wallet_kind: WalletKind::Descriptor,
            wallet_label: String::new(),
            wallet_value: String::new(),
        }
    }

    fn next(&mut self) -> Option<NodeConfig> {
        self.error = None;
        if let Err(error) = self.validate_current_step() {
            self.error = Some(error);
            return None;
        }

        if self.step == STEPS.len() - 1 {
            return Some(self.draft.clone());
        }

        self.step += 1;
        self.furthest_step = self.furthest_step.max(self.step);
        None
    }

    fn validate_current_step(&self) -> Result<(), String> {
        match self.step {
            1 | 3 | 4 => self.draft.validate(),
            2 if !self.wallet_value.trim().is_empty() => {
                Err("Add the wallet you entered, or clear the field to continue.".to_owned())
            }
            _ => Ok(()),
        }
    }

    fn back(&mut self) {
        if self.step > 0 {
            self.step -= 1;
            self.error = None;
        }
    }
}

pub fn show(
    ctx: &egui::Context,
    state: &mut SetupState,
    dark_mode: &mut bool,
) -> Option<NodeConfig> {
    let mut finished = None;
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
                        ui.set_max_width(ui.available_width().min(940.0));
                        ui.horizontal(|ui| {
                            components::brand(ui, false);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
                        ui.add_space(24.0);
                        stepper(ui, state);
                        ui.add_space(18.0);

                        components::card(ui, |ui| {
                            ui.set_min_height(430.0);
                            match state.step {
                                0 => network_step(ui, state),
                                1 => storage_step(ui, state),
                                2 => wallets_step(ui, state),
                                3 => services_step(ui, state),
                                4 => review_step(ui, state),
                                _ => unreachable!("setup step stays in bounds"),
                            }

                            if let Some(error) = &state.error {
                                ui.add_space(12.0);
                                components::inline_error(ui, error);
                            }
                        });

                        ui.add_space(14.0);
                        navigation(ui, state, &mut finished);
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("Your node stays on this computer. Floresta never stores private keys.")
                                .size(12.0)
                                .color(theme::text_muted()),
                        );
                    });
                });
        });
    finished
}

fn stepper(ui: &mut Ui, state: &mut SetupState) {
    if ui.available_width() < 700.0 {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Step {} of {}", state.step + 1, STEPS.len()))
                    .color(theme::green_dark())
                    .strong(),
            );
            ui.label(RichText::new("·").color(theme::text_muted()));
            ui.label(
                RichText::new(STEPS[state.step])
                    .color(theme::text())
                    .strong(),
            );
        });
        components::progress_bar(ui, (state.step + 1) as f32 / STEPS.len() as f32);
        return;
    }

    ui.horizontal(|ui| {
        let width = (ui.available_width() - 32.0) / STEPS.len() as f32;
        for (index, name) in STEPS.iter().enumerate() {
            let active = index == state.step;
            let visited = index <= state.furthest_step;
            let label = if index < state.step {
                format!("Done  {name}")
            } else {
                format!("{}  {name}", index + 1)
            };
            let response = ui.add_sized(
                [width, 38.0],
                egui::Button::new(
                    RichText::new(label)
                        .color(if active {
                            theme::green_dark()
                        } else {
                            theme::text_muted()
                        })
                        .strong(),
                )
                .fill(if active {
                    theme::green_pale()
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(7)),
            );
            if response.clicked() && visited {
                state.step = index;
                state.error = None;
            }
        }
    });
}

fn network_step(ui: &mut Ui, state: &mut SetupState) {
    components::page_title(
        ui,
        "Choose a Bitcoin network",
        "Mainnet is real bitcoin. Test networks are for development.",
    );
    ui.add_space(20.0);

    for network in NodeNetwork::ALL {
        let selected = state.draft.network == network;
        let response = egui::Frame::new()
            .fill(if selected {
                theme::green_pale()
            } else {
                theme::surface()
            })
            .stroke(Stroke::new(
                if selected { 1.5_f32 } else { 1.0_f32 },
                if selected {
                    theme::green()
                } else {
                    theme::border()
                },
            ))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(16, 13))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (circle, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::hover());
                    ui.painter().circle_stroke(
                        circle.center(),
                        7.0,
                        Stroke::new(
                            1.5_f32,
                            if selected {
                                theme::green()
                            } else {
                                theme::text_muted()
                            },
                        ),
                    );
                    if selected {
                        ui.painter()
                            .circle_filled(circle.center(), 4.0, theme::green());
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new(network.title()).color(theme::text()).strong());
                        ui.label(RichText::new(network.description()).color(theme::text_muted()));
                    });
                });
            })
            .response
            .interact(Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                selected,
                network.title(),
            )
        });
        if response.clicked() {
            state.draft.select_network(network);
        }
        ui.add_space(7.0);
    }

    ui.add_space(10.0);
    components::muted_card(ui, |ui| {
        components::toggle_setting(
            ui,
            &mut state.draft.use_tor,
            "Route connections through Tor",
            "Keep your node's network address private. Tor must already be running.",
        );
        if state.draft.use_tor {
            ui.add_space(8.0);
            if components::text_button(
                ui,
                if state.show_network_advanced {
                    "Hide proxy address"
                } else {
                    "Change proxy address"
                },
            )
            .clicked()
            {
                state.show_network_advanced = !state.show_network_advanced;
            }
            if state.show_network_advanced {
                labeled_text(
                    ui,
                    "SOCKS5 proxy",
                    &mut state.draft.tor_proxy,
                    "127.0.0.1:9050",
                );
            }
        }
    });
}

fn storage_step(ui: &mut Ui, state: &mut SetupState) {
    components::page_title(
        ui,
        "Choose where Floresta stores data",
        "Floresta verifies Bitcoin while keeping disk usage compact.",
    );
    ui.add_space(24.0);
    ui.label(RichText::new("Data folder").color(theme::text()).strong());
    ui.horizontal(|ui| {
        let button_width = 102.0;
        ui.add_sized(
            [ui.available_width() - button_width - 10.0, 42.0],
            TextEdit::singleline(&mut state.draft.data_dir).hint_text("Choose a writable folder"),
        );
        if ui
            .add_sized([button_width, 42.0], egui::Button::new("Browse"))
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_directory(&state.draft.data_dir)
                .pick_folder()
            {
                state.draft.data_dir = path.to_string_lossy().into_owned();
            }
        }
    });
    ui.label(
        RichText::new("Chain state, wallet index and logs are kept here.")
            .size(13.0)
            .color(theme::text_muted()),
    );

    ui.add_space(22.0);
    components::muted_card(ui, |ui| {
        components::card_title(ui, "Compact by design");
        ui.label(
            RichText::new(
                "Floresta verifies every accepted block and immediately prunes raw block data.",
            )
            .color(theme::text_muted()),
        );
    });

    ui.add_space(18.0);
    if components::text_button(
        ui,
        if state.show_sync_advanced {
            "Hide advanced sync settings"
        } else {
            "Advanced sync settings"
        },
    )
    .clicked()
    {
        state.show_sync_advanced = !state.show_sync_advanced;
    }
    if state.show_sync_advanced {
        components::muted_card(ui, |ui| {
            components::toggle_setting(
                ui,
                &mut state.draft.swift_sync,
                "Swift Sync",
                "Use hints to speed up initial sync. Turn off to skip downloads and set existing hints aside.",
            );
            ui.add_enabled_ui(state.draft.swift_sync, |ui| {
                labeled_text(
                    ui,
                    "Hints file origin (base URL)",
                    &mut state.draft.hints_origin,
                    "https://utxohints.store/hints",
                );
                ui.label(
                    RichText::new("Adds /bitcoin or /signet. Existing hints are reused; this URL applies to new downloads.")
                        .size(12.0)
                        .color(theme::text_muted()),
                );
            });
            ui.add_space(14.0);
            components::divider(ui);
            ui.add_space(14.0);
            components::toggle_setting(
                ui,
                &mut state.draft.fast_start,
                "Fast start",
                "Use a reviewed Utreexo snapshot, then validate earlier blocks in the background.",
            );
            if state.draft.fast_start {
                ui.add_space(14.0);
                components::divider(ui);
                ui.add_space(14.0);
                components::toggle_setting(
                    ui,
                    &mut state.draft.backfill,
                    "Background validation",
                    "Validate the skipped history after the node becomes usable.",
                );
            }
        });
    }
}

fn wallets_step(ui: &mut Ui, state: &mut SetupState) {
    components::page_title(
        ui,
        "Add your wallets",
        "Floresta watches balances and activity without ever holding private keys.",
    );
    ui.add_space(20.0);

    if state.draft.wallets.is_empty() {
        components::muted_card(ui, |ui| {
            components::card_title(ui, "No wallets added yet");
            ui.label(
                RichText::new("You can skip this step and add a wallet later.")
                    .color(theme::text_muted()),
            );
        });
    } else {
        let mut remove = None;
        for (index, wallet) in state.draft.wallets.iter().enumerate() {
            components::muted_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(if wallet.label.trim().is_empty() {
                                wallet.kind.title()
                            } else {
                                &wallet.label
                            })
                            .color(theme::text())
                            .strong(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                wallet.kind.title(),
                                shorten(&wallet.value, 44)
                            ))
                            .size(13.0)
                            .color(theme::text_muted()),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if components::text_button(ui, "Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                });
            });
            ui.add_space(8.0);
        }
        if let Some(index) = remove {
            state.draft.wallets.remove(index);
        }
    }

    ui.add_space(16.0);
    components::card_title(ui, "Add a watch-only wallet");
    ui.horizontal(|ui| {
        for kind in [WalletKind::Descriptor, WalletKind::ExtendedPublicKey] {
            let selected = state.wallet_kind == kind;
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(kind.title())
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
                        theme::surface()
                    })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if selected {
                            theme::green()
                        } else {
                            theme::border()
                        },
                    ))
                    .corner_radius(CornerRadius::same(7)),
                )
                .clicked()
            {
                state.wallet_kind = kind;
                state.error = None;
            }
        }
    });
    labeled_text(ui, "Name (optional)", &mut state.wallet_label, "Savings");
    ui.label(
        RichText::new(state.wallet_kind.title())
            .color(theme::text())
            .strong(),
    );
    ui.add_sized(
        [ui.available_width(), 72.0],
        TextEdit::multiline(&mut state.wallet_value).hint_text(match state.wallet_kind {
            WalletKind::Descriptor => "wpkh([fingerprint/path]xpub.../0/*)",
            WalletKind::ExtendedPublicKey => "xpub...",
        }),
    );
    ui.label(
        RichText::new("Public wallet information only. Never paste a seed phrase or private key.")
            .size(13.0)
            .color(theme::text_muted()),
    );
    ui.add_space(8.0);
    if components::secondary_button(ui, "Add wallet").clicked() {
        let wallet = WalletImport {
            label: state.wallet_label.trim().to_owned(),
            kind: state.wallet_kind,
            value: state.wallet_value.trim().to_owned(),
        };
        match validate_wallet(&wallet) {
            Ok(()) => {
                state.draft.wallets.push(wallet);
                state.wallet_label.clear();
                state.wallet_value.clear();
                state.error = None;
            }
            Err(error) => state.error = Some(error),
        }
    }
}

fn services_step(ui: &mut Ui, state: &mut SetupState) {
    components::page_title(
        ui,
        "Connect wallets and apps",
        "The local Electrum service is always available.",
    );
    ui.add_space(22.0);

    components::muted_card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Electrum server")
                        .color(theme::text())
                        .strong(),
                );
                ui.label(
                    RichText::new("Lets compatible wallets use this node on your computer.")
                        .color(theme::text_muted()),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                components::status_pill(ui, "ON", theme::green_dark(), theme::green_pale());
            });
        });
        ui.add_space(10.0);
        labeled_text(
            ui,
            "Local endpoint",
            &mut state.draft.electrum_address,
            "127.0.0.1:50001",
        );
    });

    ui.add_space(14.0);
    components::muted_card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Wallet index").color(theme::text()).strong());
                ui.label(
                    RichText::new(
                        "Required for your watch-only wallets and enabled automatically.",
                    )
                    .color(theme::text_muted()),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                components::status_pill(ui, "ON", theme::green_dark(), theme::green_pale());
            });
        });
    });

    ui.add_space(16.0);
    if components::text_button(
        ui,
        if state.show_service_advanced {
            "Hide advanced services"
        } else {
            "Advanced services"
        },
    )
    .clicked()
    {
        state.show_service_advanced = !state.show_service_advanced;
    }

    if state.show_service_advanced {
        components::muted_card(ui, |ui| {
            components::toggle_setting(
                ui,
                &mut state.draft.electrum_tls_enabled,
                "Electrum over TLS",
                "Create a self-signed certificate for encrypted wallet connections.",
            );
            if state.draft.electrum_tls_enabled {
                labeled_text(
                    ui,
                    "TLS endpoint",
                    &mut state.draft.electrum_tls_address,
                    "127.0.0.1:50002",
                );
            }
            ui.add_space(14.0);
            components::divider(ui);
            ui.add_space(14.0);
            components::toggle_setting(
                ui,
                &mut state.draft.rpc_enabled,
                "JSON-RPC for other apps",
                "Expose Floresta's developer API. Leave this off unless an app requires it.",
            );
            if state.draft.rpc_enabled {
                labeled_text(
                    ui,
                    "RPC endpoint",
                    &mut state.draft.rpc_address,
                    "127.0.0.1:8332",
                );
            }
        });
    }
}

fn review_step(ui: &mut Ui, state: &mut SetupState) {
    components::page_title(
        ui,
        "Ready to start your node",
        "Review the essentials. You can change these settings later.",
    );
    ui.add_space(22.0);

    components::muted_card(ui, |ui| {
        review_row(ui, "Network", state.draft.network.title());
        components::divider(ui);
        review_row(ui, "Data folder", &state.draft.data_dir);
        components::divider(ui);
        review_row(
            ui,
            "Sync",
            if state.draft.fast_start {
                if state.draft.backfill {
                    "Fast start with background validation"
                } else {
                    "Fast start"
                }
            } else {
                "Full validation from genesis"
            },
        );
        components::divider(ui);
        review_row(
            ui,
            "Swift Sync",
            if state.draft.swift_sync {
                "Enabled"
            } else {
                "Disabled"
            },
        );
        if state.draft.swift_sync {
            review_row(ui, "Hints origin", &state.draft.hints_origin);
        }
        components::divider(ui);
        review_row(
            ui,
            "Privacy",
            if state.draft.use_tor {
                "Tor"
            } else {
                "Direct connection"
            },
        );
        components::divider(ui);
        review_row(ui, "Wallets", &state.draft.wallet_summary());
        components::divider(ui);
        let mut services = vec!["Electrum"];
        if state.draft.rpc_enabled {
            services.push("JSON-RPC");
        }
        review_row(ui, "Services", &services.join(", "));
    });

    ui.add_space(18.0);
    egui::Frame::new()
        .fill(theme::blue_pale())
        .stroke(Stroke::new(1.0_f32, theme::blue().linear_multiply(0.25)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.label(
                RichText::new("What happens next")
                    .color(theme::blue())
                    .strong(),
            );
            ui.label(
                RichText::new(
                    "Floresta will connect to peers and verify the chain. A clear progress screen stays visible until your node is ready.",
                )
                .color(theme::text()),
            );
        });
}

fn navigation(ui: &mut Ui, state: &mut SetupState, finished: &mut Option<NodeConfig>) {
    ui.horizontal(|ui| {
        if state.step > 0 && components::secondary_button(ui, "Back").clicked() {
            state.back();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = if state.step == STEPS.len() - 1 {
                "Finish & start"
            } else {
                "Continue"
            };
            if components::primary_button(ui, label).clicked() {
                *finished = state.next();
            }
        });
    });
}

fn labeled_text(ui: &mut Ui, label: &str, value: &mut String, hint: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(label).size(13.0).color(theme::text_muted()));
    ui.add_sized(
        [ui.available_width(), 40.0],
        TextEdit::singleline(value).hint_text(hint),
    );
}

fn review_row(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::text_muted()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(shorten(value, 54))
                    .color(theme::text())
                    .strong(),
            );
        });
    });
    ui.add_space(7.0);
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
    fn shortened_wallet_material_keeps_both_ends() {
        let value = "wpkh(a-very-long-public-wallet-descriptor-with-useful-ending)";
        let shortened = shorten(value, 24);
        assert!(shortened.starts_with("wpkh("));
        assert!(shortened.ends_with("ding)"));
        assert!(shortened.chars().count() <= 24);
    }

    #[test]
    fn complete_setup_flow_renders_every_step() {
        let data_dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();

        for initial_dark_mode in [false, true] {
            theme::apply(&ctx, initial_dark_mode);
            for width in [1180.0, 680.0] {
                let mut dark_mode = initial_dark_mode;
                let mut state = SetupState::new(NodeConfig {
                    data_dir: data_dir.path().to_string_lossy().into_owned(),
                    ..NodeConfig::default()
                });
                for expected_step in 0..STEPS.len() {
                    assert_eq!(state.step, expected_step);
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 780.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            let _ = show(ctx, &mut state, &mut dark_mode);
                        },
                    );
                    assert!(
                        !output.shapes.is_empty(),
                        "setup step {expected_step} rendered no shapes at {width}px"
                    );
                    assert_eq!(dark_mode, initial_dark_mode);

                    if expected_step + 1 < STEPS.len() {
                        assert!(state.next().is_none());
                    }
                }

                assert!(state.next().is_some());
            }
        }
    }
}
