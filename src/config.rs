// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;

use bitcoin::Network;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum NodeNetwork {
    #[default]
    Mainnet,
    Testnet4,
    Signet,
    Regtest,
}

impl NodeNetwork {
    pub const ALL: [Self; 4] = [Self::Mainnet, Self::Testnet4, Self::Signet, Self::Regtest];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Mainnet => "Mainnet",
            Self::Testnet4 => "Testnet4",
            Self::Signet => "Signet",
            Self::Regtest => "Regtest",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Mainnet => "Real bitcoin and the public network",
            Self::Testnet4 => "A public network for application testing",
            Self::Signet => "A predictable network for development",
            Self::Regtest => "A private network on this computer",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Mainnet => "bitcoin",
            Self::Testnet4 => "testnet4",
            Self::Signet => "signet",
            Self::Regtest => "regtest",
        }
    }

    pub const fn bitcoin(self) -> Network {
        match self {
            Self::Mainnet => Network::Bitcoin,
            Self::Testnet4 => Network::Testnet4,
            Self::Signet => Network::Signet,
            Self::Regtest => Network::Regtest,
        }
    }

    const fn electrum_port(self) -> u16 {
        match self {
            Self::Mainnet => 50_001,
            Self::Testnet4 => 40_001,
            Self::Signet => 60_001,
            Self::Regtest => 20_001,
        }
    }

    const fn electrum_tls_port(self) -> u16 {
        match self {
            Self::Mainnet => 50_002,
            Self::Testnet4 => 40_003,
            Self::Signet => 60_002,
            Self::Regtest => 20_002,
        }
    }

    const fn rpc_port(self) -> u16 {
        match self {
            Self::Mainnet => 8_332,
            Self::Testnet4 => 48_332,
            Self::Signet => 38_332,
            Self::Regtest => 18_442,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum WalletKind {
    #[default]
    Descriptor,
    ExtendedPublicKey,
}

impl WalletKind {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Descriptor => "Descriptor",
            Self::ExtendedPublicKey => "Extended public key",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WalletImport {
    pub label: String,
    pub kind: WalletKind,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct NodeConfig {
    pub network: NodeNetwork,
    pub data_dir: String,
    pub use_tor: bool,
    pub tor_proxy: String,
    pub fast_start: bool,
    pub backfill: bool,
    pub wallets: Vec<WalletImport>,
    pub electrum_address: String,
    pub electrum_tls_enabled: bool,
    pub electrum_tls_address: String,
    pub rpc_enabled: bool,
    pub rpc_address: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".floresta")
            .to_string_lossy()
            .into_owned();

        Self {
            network: NodeNetwork::Mainnet,
            data_dir,
            use_tor: false,
            tor_proxy: "127.0.0.1:9050".to_owned(),
            fast_start: false,
            backfill: true,
            wallets: Vec::new(),
            electrum_address: "127.0.0.1:50001".to_owned(),
            electrum_tls_enabled: false,
            electrum_tls_address: "127.0.0.1:50002".to_owned(),
            rpc_enabled: false,
            rpc_address: "127.0.0.1:8332".to_owned(),
        }
    }
}

impl NodeConfig {
    pub fn node_data_dir(&self) -> PathBuf {
        let base = PathBuf::from(&self.data_dir);
        match self.network {
            NodeNetwork::Mainnet => base,
            network => base.join(network.slug()),
        }
    }

    pub fn select_network(&mut self, network: NodeNetwork) {
        let previous = self.network;
        replace_default_endpoint(
            &mut self.electrum_address,
            previous.electrum_port(),
            network.electrum_port(),
        );
        replace_default_endpoint(
            &mut self.electrum_tls_address,
            previous.electrum_tls_port(),
            network.electrum_tls_port(),
        );
        replace_default_endpoint(
            &mut self.rpc_address,
            previous.rpc_port(),
            network.rpc_port(),
        );
        self.network = network;
    }

    pub fn validate(&self) -> Result<(), String> {
        self.validate_data_dir()?;

        if self.use_tor {
            validate_endpoint("Tor proxy", &self.tor_proxy)?;
        }
        validate_endpoint("Electrum endpoint", &self.electrum_address)?;
        if self.electrum_tls_enabled {
            validate_endpoint("Electrum TLS endpoint", &self.electrum_tls_address)?;
        }
        if self.rpc_enabled {
            validate_endpoint("RPC endpoint", &self.rpc_address)?;
        }

        for wallet in &self.wallets {
            validate_wallet(wallet)?;
        }

        Ok(())
    }

    fn validate_data_dir(&self) -> Result<(), String> {
        let path = Path::new(self.data_dir.trim());
        if self.data_dir.trim().is_empty() {
            return Err("Choose a folder for Floresta's data.".to_owned());
        }

        fs::create_dir_all(path)
            .map_err(|error| format!("Could not create the data folder: {error}"))?;

        let probe = path.join(format!(".floresta-write-test-{}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&probe) {
            Ok(_) => {
                let _ = fs::remove_file(probe);
                Ok(())
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(format!("Folder not writable: {error}")),
        }
    }

    pub fn wallet_summary(&self) -> String {
        match self.wallets.len() {
            0 => "No watch-only wallets".to_owned(),
            1 => "1 watch-only wallet".to_owned(),
            count => format!("{count} watch-only wallets"),
        }
    }
}

fn replace_default_endpoint(endpoint: &mut String, previous_port: u16, new_port: u16) {
    if endpoint == &format!("127.0.0.1:{previous_port}") {
        *endpoint = format!("127.0.0.1:{new_port}");
    }
}

pub fn validate_wallet(wallet: &WalletImport) -> Result<(), String> {
    let value = wallet.value.trim();
    if value.is_empty() {
        return Err(format!("Enter a {}.", wallet.kind.title().to_lowercase()));
    }
    let lower = value.to_ascii_lowercase();
    const PRIVATE_KEY_MARKERS: [&str; 6] = ["xprv", "tprv", "yprv", "zprv", "uprv", "vprv"];
    if PRIVATE_KEY_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return Err("Private keys are not allowed. Floresta wallets are watch-only.".to_owned());
    }

    match wallet.kind {
        WalletKind::Descriptor if !(value.contains('(') && value.contains(')')) => {
            Err("That does not look like an output descriptor.".to_owned())
        }
        WalletKind::ExtendedPublicKey
            if !["xpub", "tpub", "ypub", "zpub", "upub", "vpub"]
                .iter()
                .any(|prefix| lower.starts_with(prefix)) =>
        {
            Err("That does not look like an extended public key.".to_owned())
        }
        _ => Ok(()),
    }
}

fn validate_endpoint(name: &str, endpoint: &str) -> Result<(), String> {
    let endpoint = endpoint.trim();
    let Some((host, port)) = endpoint.rsplit_once(':') else {
        return Err(format!("{name} must use host:port."));
    };

    let Ok(port) = port.parse::<u16>() else {
        return Err(format!("{name} must use a valid host:port."));
    };
    if host.is_empty() || port == 0 {
        return Err(format!("{name} must use a valid host:port."));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_data_isolated_but_mainnet_keeps_base_path() {
        let config = NodeConfig {
            data_dir: "/tmp/floresta-desktop-test".to_owned(),
            ..NodeConfig::default()
        };
        assert_eq!(config.node_data_dir(), PathBuf::from(&config.data_dir));

        let testnet = NodeConfig {
            network: NodeNetwork::Testnet4,
            ..config
        };
        assert_eq!(
            testnet.node_data_dir(),
            PathBuf::from(&testnet.data_dir).join("testnet4")
        );
    }

    #[test]
    fn selecting_network_updates_only_default_service_ports() {
        let mut config = NodeConfig::default();
        config.select_network(NodeNetwork::Signet);
        assert_eq!(config.electrum_address, "127.0.0.1:60001");
        assert_eq!(config.electrum_tls_address, "127.0.0.1:60002");
        assert_eq!(config.rpc_address, "127.0.0.1:38332");

        config.electrum_address = "192.0.2.1:50001".to_owned();
        config.select_network(NodeNetwork::Regtest);
        assert_eq!(config.electrum_address, "192.0.2.1:50001");
        assert_eq!(config.electrum_tls_address, "127.0.0.1:20002");
        assert_eq!(config.rpc_address, "127.0.0.1:18442");
    }

    #[test]
    fn watch_only_imports_reject_private_keys() {
        let wallet = WalletImport {
            label: "Savings".to_owned(),
            kind: WalletKind::Descriptor,
            value: "wpkh(xprv-example/0/*)".to_owned(),
        };
        assert_eq!(
            validate_wallet(&wallet),
            Err("Private keys are not allowed. Floresta wallets are watch-only.".to_owned())
        );
    }

    #[test]
    fn endpoint_requires_host_and_nonzero_numeric_port() {
        assert!(validate_endpoint("RPC endpoint", "127.0.0.1:8332").is_ok());
        assert!(validate_endpoint("RPC endpoint", "localhost").is_err());
        assert!(validate_endpoint("RPC endpoint", "localhost:not-a-port").is_err());
        assert!(validate_endpoint("RPC endpoint", "localhost:0").is_err());
    }
}
