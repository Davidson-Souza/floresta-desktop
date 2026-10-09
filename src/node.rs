// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::net::TcpListener;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use floresta_node::Config as FlorestaConfig;
use floresta_node::Florestad;
use floresta_rpc::jsonrpc_client::Client;
use floresta_rpc::rpc::FlorestaRPC;
use serde_json::Value;

use crate::config::NodeConfig;
use crate::config::WalletKind;

#[derive(Clone, Debug)]
pub struct PeerSnapshot {
    pub address: String,
    pub user_agent: String,
    pub state: String,
    pub transport: String,
    pub inbound: bool,
}

#[derive(Clone, Debug)]
pub struct NodeSnapshot {
    pub chain: String,
    pub block_height: u64,
    pub target_height: u64,
    pub verification_progress: f32,
    pub initial_block_download: bool,
    pub peer_count: usize,
    pub inbound_peers: usize,
    pub outbound_peers: usize,
    pub v2_peers: usize,
    pub peers: Vec<PeerSnapshot>,
    pub uptime_seconds: u64,
    pub size_on_disk: u64,
    pub last_block_time: u64,
    pub user_agent: String,
    pub received_at: Instant,
}

impl NodeSnapshot {
    pub fn is_ready(&self) -> bool {
        !self.initial_block_download && self.block_height >= self.target_height
    }

    pub fn blocks_behind(&self) -> u64 {
        self.target_height.saturating_sub(self.block_height)
    }

    pub fn phase(&self) -> &'static str {
        if self.peer_count == 0 && self.initial_block_download {
            "Finding peers"
        } else if self.block_height < self.target_height {
            "Verifying blocks"
        } else if self.initial_block_download {
            "Final checks"
        } else {
            "Ready"
        }
    }
}

#[derive(Clone, Debug)]
pub enum NodeEvent {
    Starting(String),
    Snapshot(NodeSnapshot),
    Activity(String),
    Error(String),
    Stopped,
}

enum NodeCommand {
    Start(NodeConfig),
    Stop,
    Shutdown,
}

pub struct NodeController {
    command_tx: mpsc::Sender<NodeCommand>,
    event_rx: mpsc::Receiver<NodeEvent>,
    worker: Option<thread::JoinHandle<()>>,
}

impl NodeController {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("floresta-desktop-node".to_owned())
            .spawn(move || supervisor_loop(command_rx, event_tx))
            .expect("node supervisor thread should start");

        Self {
            command_tx,
            event_rx,
            worker: Some(worker),
        }
    }

    pub fn start(&self, config: NodeConfig) -> Result<(), String> {
        self.command_tx
            .send(NodeCommand::Start(config))
            .map_err(|_| "The node controller is unavailable.".to_owned())
    }

    pub fn stop(&self) {
        let _ = self.command_tx.send(NodeCommand::Stop);
    }

    pub fn try_recv(&self) -> Option<NodeEvent> {
        self.event_rx.try_recv().ok()
    }
}

impl Drop for NodeController {
    fn drop(&mut self) {
        let _ = self.command_tx.send(NodeCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeExit {
    Stopped,
    Shutdown,
}

fn supervisor_loop(command_rx: mpsc::Receiver<NodeCommand>, event_tx: mpsc::Sender<NodeEvent>) {
    while let Ok(command) = command_rx.recv() {
        match command {
            NodeCommand::Start(config) => {
                let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    run_node(config, &command_rx, &event_tx)
                }));
                match result {
                    Ok(NodeExit::Shutdown) => break,
                    Ok(NodeExit::Stopped) => {
                        let _ = event_tx.send(NodeEvent::Stopped);
                    }
                    Err(_) => {
                        let _ = event_tx.send(NodeEvent::Error(
                            "The node stopped unexpectedly. Restart it or review the data folder."
                                .to_owned(),
                        ));
                    }
                }
            }
            NodeCommand::Shutdown => break,
            NodeCommand::Stop => {
                let _ = event_tx.send(NodeEvent::Stopped);
            }
        }
    }
}

fn run_node(
    app_config: NodeConfig,
    command_rx: &mpsc::Receiver<NodeCommand>,
    event_tx: &mpsc::Sender<NodeEvent>,
) -> NodeExit {
    let _ = event_tx.send(NodeEvent::Starting("Opening the chain database".to_owned()));

    let (node_config, rpc_url) = match build_node_config(&app_config) {
        Ok(result) => result,
        Err(error) => {
            let _ = event_tx.send(NodeEvent::Error(error));
            return NodeExit::Stopped;
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .max_blocking_threads(2)
        .thread_name("floresta-desktop-runtime")
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = event_tx.send(NodeEvent::Error(format!(
                "Could not create the node runtime: {error}"
            )));
            return NodeExit::Stopped;
        }
    };

    runtime.block_on(async move {
        let node = Florestad::from(node_config);
        if let Err(error) = node.start().await {
            let _ = event_tx.send(NodeEvent::Error(format!(
                "Could not start Floresta: {error}"
            )));
            return NodeExit::Stopped;
        }

        let _ = event_tx.send(NodeEvent::Activity(format!(
            "Node started on {}",
            app_config.network.title()
        )));
        let monitor_running = Arc::new(AtomicBool::new(true));
        let monitor = start_monitor(rpc_url, Arc::clone(&monitor_running), event_tx.clone());

        let exit = loop {
            match command_rx.try_recv() {
                Ok(NodeCommand::Stop) => break NodeExit::Stopped,
                Ok(NodeCommand::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => {
                    break NodeExit::Shutdown;
                }
                Ok(NodeCommand::Start(_)) => {
                    let _ = event_tx.send(NodeEvent::Activity(
                        "The node is already running; stop it before starting again.".to_owned(),
                    ));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }

            if node.should_stop().await {
                break NodeExit::Stopped;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        };

        monitor_running.store(false, Ordering::Release);
        let _ = monitor.join();
        node.stop().await;
        let _ = tokio::time::timeout(Duration::from_secs(10), node.wait_shutdown()).await;
        exit
    })
}

fn build_node_config(app_config: &NodeConfig) -> Result<(FlorestaConfig, String), String> {
    app_config.validate()?;
    let data_dir = app_config.node_data_dir();
    fs::create_dir_all(&data_dir)
        .map_err(|error| format!("Could not create the node data folder: {error}"))?;

    let rpc_bind = if app_config.rpc_enabled {
        app_config.rpc_address.trim().to_owned()
    } else {
        available_loopback_address()?
    };
    let rpc_url = rpc_url_for_bind(&rpc_bind)?;

    let mut config = FlorestaConfig::new(app_config.network.bitcoin(), data_dir);
    config.cfilters = true;
    config.proxy = app_config
        .use_tor
        .then(|| app_config.tor_proxy.trim().to_owned());
    config.assume_utreexo = app_config.fast_start;
    config.backfill = app_config.fast_start && app_config.backfill;
    config.wallet_descriptor = non_empty_wallets(app_config, WalletKind::Descriptor);
    config.wallet_xpub = non_empty_wallets(app_config, WalletKind::ExtendedPublicKey);
    config.json_rpc_address = Some(rpc_bind);
    config.electrum_address = Some(app_config.electrum_address.trim().to_owned());
    config.enable_electrum_tls = app_config.electrum_tls_enabled;
    config.electrum_address_tls = app_config
        .electrum_tls_enabled
        .then(|| app_config.electrum_tls_address.trim().to_owned());
    config.generate_cert = app_config.electrum_tls_enabled;
    config.log_to_file = true;
    config.log_to_stdout = false;
    config.user_agent = format!("/FlorestaDesktop:{}/", env!("CARGO_PKG_VERSION"));

    Ok((config, rpc_url))
}

fn non_empty_wallets(config: &NodeConfig, kind: WalletKind) -> Option<Vec<String>> {
    let wallets: Vec<_> = config
        .wallets
        .iter()
        .filter(|wallet| wallet.kind == kind)
        .map(|wallet| wallet.value.trim().to_owned())
        .filter(|wallet| !wallet.is_empty())
        .collect();
    (!wallets.is_empty()).then_some(wallets)
}

fn available_loopback_address() -> Result<String, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Could not reserve a local control port: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Could not read the local control port: {error}"))?
        .port();
    Ok(format!("127.0.0.1:{port}"))
}

fn rpc_url_for_bind(bind: &str) -> Result<String, String> {
    let Some((host, port)) = bind.trim().rsplit_once(':') else {
        return Err("RPC endpoint must use host:port.".to_owned());
    };
    let host = match host {
        "0.0.0.0" | "[::]" | "::" => "127.0.0.1",
        other => other,
    };
    Ok(format!("http://{host}:{port}"))
}

fn start_monitor(
    rpc_url: String,
    running: Arc<AtomicBool>,
    event_tx: mpsc::Sender<NodeEvent>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("floresta-desktop-status".to_owned())
        .spawn(move || {
            let client = Client::new(rpc_url);
            let mut attempts = 0_u32;
            while running.load(Ordering::Acquire) {
                match fetch_snapshot(&client) {
                    Ok(snapshot) => {
                        attempts = 0;
                        if event_tx.send(NodeEvent::Snapshot(snapshot)).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        attempts = attempts.saturating_add(1);
                        if attempts == 1 {
                            let _ = event_tx.send(NodeEvent::Starting(
                                "Connecting to the local node".to_owned(),
                            ));
                        } else if attempts == 2 {
                            let _ = event_tx.send(NodeEvent::Activity(format!(
                                "Waiting for the local status API: {error}"
                            )));
                        } else if attempts == 8 {
                            let _ = event_tx.send(NodeEvent::Error(format!(
                                "The node is running, but its status is unavailable: {error}"
                            )));
                        }
                    }
                }

                for _ in 0..20 {
                    if !running.load(Ordering::Acquire) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        })
        .expect("status monitor thread should start")
}

fn fetch_snapshot(client: &Client) -> Result<NodeSnapshot, String> {
    let chain = client
        .rpc_call::<Value>("getblockchaininfo", &[])
        .map_err(|error| format!("{error:?}"))?;
    let network = client
        .rpc_call::<Value>("getnetworkinfo", &[])
        .unwrap_or(Value::Null);
    let peers = client
        .rpc_call::<Value>("getpeerinfo", &[])
        .unwrap_or_else(|_| Value::Array(Vec::new()));
    let uptime = client.uptime().map(u64::from).unwrap_or(0);

    snapshot_from_values(&chain, &network, &peers, uptime)
}

fn snapshot_from_values(
    chain: &Value,
    network: &Value,
    peers: &Value,
    uptime_seconds: u64,
) -> Result<NodeSnapshot, String> {
    let block_height = required_u64(chain, "blocks")?;
    let target_height = required_u64(chain, "headers")?;
    let initial_block_download = chain
        .get("initialblockdownload")
        .and_then(Value::as_bool)
        .unwrap_or(block_height < target_height);
    let verification_progress = chain
        .get("verificationprogress")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            if target_height == 0 {
                0.0
            } else {
                block_height as f64 / target_height as f64
            }
        })
        .clamp(0.0, 1.0) as f32;

    let peer_values = peers.as_array().map(Vec::as_slice).unwrap_or_default();
    let inbound_peers = peer_values
        .iter()
        .filter(|peer| peer.get("inbound").and_then(Value::as_bool) == Some(true))
        .count();
    let v2_peers = peer_values
        .iter()
        .filter(|peer| {
            peer.get("transport_protocol")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("v2"))
        })
        .count();
    let peer_details = peer_values
        .iter()
        .map(|peer| PeerSnapshot {
            address: peer
                .get("address")
                .and_then(Value::as_str)
                .unwrap_or("Unknown address")
                .to_owned(),
            user_agent: peer
                .get("user_agent")
                .and_then(Value::as_str)
                .unwrap_or("Unknown client")
                .to_owned(),
            state: peer
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("Unknown")
                .to_owned(),
            transport: peer
                .get("transport_protocol")
                .and_then(Value::as_str)
                .unwrap_or("V1")
                .to_owned(),
            inbound: peer
                .get("inbound")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .collect();
    let peer_count = network
        .get("connections")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or(peer_values.len());

    Ok(NodeSnapshot {
        chain: chain
            .get("chain")
            .and_then(Value::as_str)
            .unwrap_or("bitcoin")
            .to_owned(),
        block_height,
        target_height,
        verification_progress,
        initial_block_download,
        peer_count,
        inbound_peers,
        outbound_peers: peer_count.saturating_sub(inbound_peers),
        v2_peers,
        peers: peer_details,
        uptime_seconds,
        size_on_disk: chain
            .get("size_on_disk")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        last_block_time: chain.get("time").and_then(Value::as_u64).unwrap_or(0),
        user_agent: network
            .get("subversion")
            .and_then(Value::as_str)
            .unwrap_or("/Floresta/")
            .to_owned(),
        received_at: Instant::now(),
    })
}

fn required_u64(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Node status did not include {field}."))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::NodeNetwork;
    use crate::config::WalletImport;

    #[test]
    fn snapshot_uses_live_chain_and_peer_values() {
        let chain = json!({
            "blocks": 420,
            "headers": 1_000,
            "initialblockdownload": true,
            "verificationprogress": 0.42,
            "chain": "signet",
            "size_on_disk": 5_000,
            "time": 1_700_000_000_u64
        });
        let network = json!({ "connections": 2, "subversion": "/Floresta:0.9.0/" });
        let peers = json!([
            { "inbound": false, "transport_protocol": "V2" },
            { "inbound": true, "transport_protocol": "V1" }
        ]);

        let snapshot = snapshot_from_values(&chain, &network, &peers, 30).unwrap();
        assert_eq!(snapshot.block_height, 420);
        assert_eq!(snapshot.blocks_behind(), 580);
        assert_eq!(snapshot.peer_count, 2);
        assert_eq!(snapshot.inbound_peers, 1);
        assert_eq!(snapshot.v2_peers, 1);
        assert_eq!(snapshot.phase(), "Verifying blocks");
    }

    #[test]
    fn node_configuration_keeps_supported_choices() {
        let data_dir = tempfile::tempdir().unwrap();
        let app_config = NodeConfig {
            data_dir: data_dir.path().to_string_lossy().into_owned(),
            use_tor: true,
            fast_start: true,
            backfill: true,
            rpc_enabled: true,
            wallets: vec![WalletImport {
                label: "Savings".to_owned(),
                kind: WalletKind::ExtendedPublicKey,
                value: "xpub-example".to_owned(),
            }],
            ..NodeConfig::default()
        };

        let (node_config, rpc_url) = build_node_config(&app_config).unwrap();
        assert_eq!(node_config.proxy.as_deref(), Some("127.0.0.1:9050"));
        assert!(node_config.assume_utreexo);
        assert!(node_config.backfill);
        assert_eq!(
            node_config.electrum_address.as_deref(),
            Some("127.0.0.1:50001")
        );
        assert_eq!(
            node_config.wallet_xpub,
            Some(vec!["xpub-example".to_owned()])
        );
        assert_eq!(rpc_url, "http://127.0.0.1:8332");
    }

    #[test]
    fn wildcard_rpc_bind_uses_loopback_for_the_desktop_client() {
        assert_eq!(
            rpc_url_for_bind("0.0.0.0:8332"),
            Ok("http://127.0.0.1:8332".to_owned())
        );
    }

    #[test]
    #[ignore = "starts a real node and can take several minutes in debug builds"]
    fn embedded_regtest_reaches_live_status() {
        let data_dir = tempfile::tempdir().unwrap();
        let electrum_address = available_loopback_address().unwrap();
        let controller = NodeController::new();
        controller
            .start(NodeConfig {
                network: NodeNetwork::Regtest,
                data_dir: data_dir.path().to_string_lossy().into_owned(),
                electrum_address,
                ..NodeConfig::default()
            })
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(180);
        let mut activity = Vec::new();
        loop {
            if let Some(event) = controller.try_recv() {
                match event {
                    NodeEvent::Snapshot(snapshot) => {
                        assert_eq!(snapshot.chain, "regtest");
                        break;
                    }
                    NodeEvent::Error(error) => panic!("embedded node failed: {error}"),
                    NodeEvent::Starting(detail) | NodeEvent::Activity(detail) => {
                        activity.push(detail);
                    }
                    NodeEvent::Stopped => activity.push("Node stopped".to_owned()),
                }
            }
            assert!(
                Instant::now() < deadline,
                "embedded node did not publish status within 180 seconds; events: {activity:?}"
            );
            thread::sleep(Duration::from_millis(100));
        }

        controller.stop();
    }
}
