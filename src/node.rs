// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::net::TcpListener;
use std::panic::AssertUnwindSafe;
use std::path::Path;
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
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::config::NodeConfig;
use crate::config::NodeNetwork;
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
    /// Live IBD height from Prometheus; it may be ahead of the committed chain.
    pub sync_height: u64,
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
        self.target_height.saturating_sub(self.sync_height)
    }

    pub fn phase(&self) -> &'static str {
        if self.peer_count == 0 && self.initial_block_download {
            "Finding peers"
        } else if self.sync_height < self.target_height {
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

#[derive(Clone)]
struct NodeEventSender {
    sender: mpsc::Sender<NodeEvent>,
    repaint: eframe::egui::Context,
}

impl NodeEventSender {
    fn channel(repaint: eframe::egui::Context) -> (Self, mpsc::Receiver<NodeEvent>) {
        let (sender, receiver) = mpsc::channel();
        (Self { sender, repaint }, receiver)
    }

    fn send(&self, event: NodeEvent) -> Result<(), ()> {
        self.sender.send(event).map_err(|_| ())?;
        // Queue the event before waking the UI, so it can drain the channel immediately.
        self.repaint.request_repaint();
        Ok(())
    }
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
    pub fn new(repaint: eframe::egui::Context) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = NodeEventSender::channel(repaint);
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

fn supervisor_loop(command_rx: mpsc::Receiver<NodeCommand>, event_tx: NodeEventSender) {
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
    event_tx: &NodeEventSender,
) -> NodeExit {
    let _ = event_tx.send(NodeEvent::Starting("Preparing the node".to_owned()));

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
        let hints = prepare_hints(&app_config, event_tx);
        tokio::pin!(hints);
        loop {
            tokio::select! {
                result = &mut hints => {
                    if let Err(error) = result {
                        let _ = event_tx.send(NodeEvent::Error(error));
                        return NodeExit::Stopped;
                    }
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    match command_rx.try_recv() {
                        Ok(NodeCommand::Stop) => return NodeExit::Stopped,
                        Ok(NodeCommand::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => {
                            return NodeExit::Shutdown;
                        }
                        Ok(NodeCommand::Start(_)) | Err(mpsc::TryRecvError::Empty) => {}
                    }
                }
            }
        }

        let metrics_address = match available_loopback_address() {
            Ok(address) => address,
            Err(error) => {
                let _ = event_tx.send(NodeEvent::Error(error));
                return NodeExit::Stopped;
            }
        };
        // The registry is process-global, so clear the previous node's height on restart.
        floresta_metrics::get_metrics().block_height.set(0);
        tokio::spawn(floresta_metrics::metrics_server(
            metrics_address
                .parse()
                .expect("allocated loopback address is valid"),
        ));
        let metrics_url = format!("http://{metrics_address}/");

        let _ = event_tx.send(NodeEvent::Starting("Opening the chain database".to_owned()));
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
        let monitor = start_monitor(
            rpc_url,
            metrics_url,
            Arc::clone(&monitor_running),
            event_tx.clone(),
        );

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

async fn prepare_hints(config: &NodeConfig, event_tx: &NodeEventSender) -> Result<(), String> {
    let network = config.network.slug();
    let path = config.node_data_dir().join(format!("{network}.hints"));
    let disabled_path = path.with_extension("hints.disabled");
    let (source, destination) = if config.swift_sync {
        (&disabled_path, &path)
    } else {
        (&path, &disabled_path)
    };
    if source
        .try_exists()
        .map_err(|error| format!("Could not check {}: {error}", source.display()))?
    {
        let destination_exists = destination
            .try_exists()
            .map_err(|error| format!("Could not check {}: {error}", destination.display()))?;
        if !destination_exists {
            fs::rename(source, destination)
                .map_err(|error| format!("Could not move {}: {error}", source.display()))?;
        } else if !config.swift_sync {
            return Err(format!(
                "Cannot disable Swift Sync while both {} and {} exist. Move one aside and try again.",
                path.display(),
                disabled_path.display(),
            ));
        }
    }
    if !config.swift_sync {
        return Ok(());
    }

    if !matches!(config.network, NodeNetwork::Mainnet | NodeNetwork::Signet) {
        return Ok(());
    }

    if path
        .try_exists()
        .map_err(|error| format!("Could not check {}: {error}", path.display()))?
    {
        return Ok(());
    }

    let _ = event_tx.send(NodeEvent::Starting(format!("Downloading {network} hints")));
    let mut client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(30));
    if config.use_tor {
        let proxy = reqwest::Proxy::all(format!("socks5h://{}", config.tor_proxy.trim()))
            .map_err(|error| format!("Could not configure the hints download proxy: {error}"))?;
        client = client.proxy(proxy);
    }
    let client = client
        .build()
        .map_err(|error| format!("Could not create the hints download client: {error}"))?;
    download_hints(&client, config.hints_url()?.as_str(), &path)
        .await
        .map_err(|error| format!("Could not download {network} hints: {error}"))
}

async fn download_hints(
    client: &reqwest::Client,
    url: &str,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    // Keep incomplete downloads out of Floresta's hints loader, including on cancellation.
    let temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    let mut file = tokio::fs::File::from_std(temporary.reopen()?);
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    temporary.persist(path)?;
    Ok(())
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
    metrics_url: String,
    running: Arc<AtomicBool>,
    event_tx: NodeEventSender,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("floresta-desktop-status".to_owned())
        .spawn(move || {
            let client = Client::new(rpc_url);
            let metrics_client = match reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
            {
                Ok(client) => client,
                Err(error) => {
                    let _ = event_tx.send(NodeEvent::Error(format!(
                        "Could not create the local metrics client: {error}"
                    )));
                    return;
                }
            };
            let mut attempts = 0_u32;
            while running.load(Ordering::Acquire) {
                match fetch_snapshot(&client, &metrics_client, &metrics_url) {
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

fn fetch_snapshot(
    client: &Client,
    metrics_client: &reqwest::blocking::Client,
    metrics_url: &str,
) -> Result<NodeSnapshot, String> {
    let chain = client
        .rpc_call::<Value>("getblockchaininfo", &[])
        .map_err(|error| format!("{error:?}"))?;
    let network = client
        .rpc_call::<Value>("getnetworkinfo", &[])
        .unwrap_or(Value::Null);
    let peers = client
        .rpc_call::<Value>("getpeerinfo", &[])
        .unwrap_or_else(|_| Value::Array(Vec::new()));
    let uptime = client
        .rpc_call::<u32>("uptime", &[])
        .map(u64::from)
        .unwrap_or(0);

    let mut snapshot = snapshot_from_values(&chain, &network, &peers, uptime)?;
    if snapshot.initial_block_download {
        let metrics = metrics_client
            .get(metrics_url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .and_then(reqwest::blocking::Response::text)
            .map_err(|error| format!("Could not read local IBD metrics: {error}"))?;
        apply_ibd_metrics(&mut snapshot, &metrics)?;
    }
    Ok(snapshot)
}

fn apply_ibd_metrics(snapshot: &mut NodeSnapshot, metrics: &str) -> Result<(), String> {
    if !snapshot.initial_block_download {
        return Ok(());
    }
    let height = metrics.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next() == Some("block_height"))
            .then(|| fields.next())
            .flatten()
    });
    let height = height
        .ok_or_else(|| "Local IBD metrics did not include block_height.".to_owned())?
        .parse::<u64>()
        .map_err(|error| format!("Invalid block_height in local IBD metrics: {error}"))?;
    snapshot.sync_height = height.max(snapshot.block_height);
    snapshot.verification_progress = if snapshot.target_height == 0 {
        0.0
    } else {
        (snapshot.sync_height as f64 / snapshot.target_height as f64).clamp(0.0, 1.0) as f32
    };
    Ok(())
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
        sync_height: block_height,
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
    fn ibd_metrics_advance_progress_without_committing_chain_or_marking_ready() {
        let chain = serde_json::json!({
            "blocks": 0,
            "headers": 1_000,
            "initialblockdownload": true,
            "verificationprogress": 0.0
        });
        let mut snapshot = snapshot_from_values(
            &chain,
            &serde_json::json!({"connections": 2}),
            &serde_json::json!([]),
            0,
        )
        .unwrap();
        apply_ibd_metrics(
            &mut snapshot,
            "# TYPE block_height gauge\nblock_height 420\n",
        )
        .unwrap();
        assert_eq!(snapshot.block_height, 0);
        assert_eq!(snapshot.sync_height, 420);
        assert_eq!(snapshot.verification_progress, 0.42);
        assert_eq!(snapshot.blocks_behind(), 580);
        assert!(!snapshot.is_ready());

        apply_ibd_metrics(&mut snapshot, "block_height 1000\n").unwrap();
        assert_eq!(snapshot.phase(), "Final checks");
        assert!(!snapshot.is_ready());

        let committed = serde_json::json!({
            "blocks": 1_000,
            "headers": 1_000,
            "initialblockdownload": false,
            "verificationprogress": 1.0
        });
        let mut snapshot = snapshot_from_values(&committed, &Value::Null, &Value::Null, 0).unwrap();
        apply_ibd_metrics(&mut snapshot, "block_height 420\n").unwrap();
        assert_eq!(snapshot.sync_height, 1_000);
        assert_eq!(snapshot.verification_progress, 1.0);
        assert!(snapshot.is_ready());
    }

    #[test]
    fn ibd_metrics_reject_missing_or_invalid_heights() {
        let chain = serde_json::json!({"blocks": 0, "headers": 100, "initialblockdownload": true});
        let mut snapshot = snapshot_from_values(&chain, &Value::Null, &Value::Null, 0).unwrap();
        for metrics in [
            "# HELP block_height Current block height\n",
            "other_block_height 20\n",
            "block_height -1\n",
            "block_height NaN\n",
            "block_height 1.5\n",
        ] {
            assert!(
                apply_ibd_metrics(&mut snapshot, metrics).is_err(),
                "{metrics}"
            );
        }
    }

    #[tokio::test]
    async fn disabled_swift_sync_parks_hints_without_network_access_and_restores_them() {
        let data_dir = tempfile::tempdir().unwrap();
        let mut config = NodeConfig {
            data_dir: data_dir.path().to_string_lossy().into_owned(),
            swift_sync: false,
            hints_origin: "not a URL".to_owned(),
            ..NodeConfig::default()
        };
        let (events, received) = NodeEventSender::channel(eframe::egui::Context::default());
        let hints = data_dir.path().join("bitcoin.hints");
        let parked = data_dir.path().join("bitcoin.hints.disabled");

        config.validate().unwrap();
        prepare_hints(&config, &events).await.unwrap();
        assert!(!hints.exists());
        assert!(!parked.exists());

        fs::write(&hints, b"previous hints").unwrap();
        prepare_hints(&config, &events).await.unwrap();
        assert!(!hints.exists());
        assert_eq!(fs::read(&parked).unwrap(), b"previous hints");
        prepare_hints(&config, &events).await.unwrap();

        config.swift_sync = true;
        prepare_hints(&config, &events).await.unwrap();
        assert_eq!(fs::read(&hints).unwrap(), b"previous hints");
        assert!(!parked.exists());
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn disabling_swift_sync_does_not_overwrite_parked_hints() {
        let data_dir = tempfile::tempdir().unwrap();
        let config = NodeConfig {
            data_dir: data_dir.path().to_string_lossy().into_owned(),
            swift_sync: false,
            ..NodeConfig::default()
        };
        let hints = data_dir.path().join("bitcoin.hints");
        let parked = data_dir.path().join("bitcoin.hints.disabled");
        fs::write(&hints, b"active").unwrap();
        fs::write(&parked, b"parked").unwrap();
        let (events, _) = NodeEventSender::channel(eframe::egui::Context::default());

        assert!(prepare_hints(&config, &events).await.is_err());
        assert_eq!(fs::read(&hints).unwrap(), b"active");
        assert_eq!(fs::read(&parked).unwrap(), b"parked");
    }

    #[tokio::test]
    async fn hints_download_uses_custom_origin_and_selected_network() {
        use tokio::io::AsyncBufReadExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}/mirror/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = tokio::io::BufReader::new(stream);
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                assert!(stream.read_until(b'\n', &mut request).await.unwrap() > 0);
            }
            assert!(request.starts_with(b"GET /mirror/signet HTTP/1.1\r\n"));
            stream
                .get_mut()
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhints")
                .await
                .unwrap();
        });
        let data_dir = tempfile::tempdir().unwrap();
        let config = NodeConfig {
            network: NodeNetwork::Signet,
            data_dir: data_dir.path().to_string_lossy().into_owned(),
            hints_origin: origin,
            ..NodeConfig::default()
        };
        build_node_config(&config).unwrap();
        let (events, _) = NodeEventSender::channel(eframe::egui::Context::default());
        prepare_hints(&config, &events).await.unwrap();
        server.await.unwrap();
        assert_eq!(
            fs::read(config.node_data_dir().join("signet.hints")).unwrap(),
            b"hints",
        );
        assert!(!data_dir.path().join("bitcoin.hints").exists());
    }

    #[tokio::test]
    async fn failed_hints_download_never_publishes_partial_data() {
        use tokio::io::AsyncBufReadExt;

        for response in [
            "HTTP/1.1 404 Not Found\r\nContent-Length: 7\r\n\r\nmissing",
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\npartial",
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/hints/signet", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = tokio::io::BufReader::new(stream);
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    assert!(stream.read_until(b'\n', &mut request).await.unwrap() > 0);
                }
                stream
                    .get_mut()
                    .write_all(response.as_bytes())
                    .await
                    .unwrap();
            });
            let data_dir = tempfile::tempdir().unwrap();
            let path = data_dir.path().join("signet.hints");
            let client = reqwest::Client::builder().no_proxy().build().unwrap();

            assert!(download_hints(&client, &url, &path).await.is_err());
            server.await.unwrap();
            assert!(!path.exists());
            assert_eq!(fs::read_dir(data_dir.path()).unwrap().count(), 0);
        }
    }

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
        let controller = NodeController::new(eframe::egui::Context::default());
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
