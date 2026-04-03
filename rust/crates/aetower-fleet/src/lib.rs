use aetower_model::SystemSnapshot;

/// A peer Mac on the local network with its latest snapshot.
#[derive(Debug, Clone)]
pub struct FleetPeer {
    pub name: String,
    pub address: String,
    pub last_snapshot: Option<SystemSnapshot>,
    pub last_seen_millis: u64,
}

/// Aggregate view of all discovered peers.
#[derive(Debug, Clone, Default)]
pub struct FleetSnapshot {
    pub peers: Vec<FleetPeer>,
}

/// Configuration for the fleet discovery and sharing system.
#[derive(Debug, Clone)]
pub struct FleetConfig {
    pub enabled: bool,
    pub listen_port: u16,
    pub service_name: String,
}

impl Default for FleetConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_port: 7780,
            service_name: "aetower".to_owned(),
        }
    }
}

/// Fleet manager that handles Bonjour discovery and snapshot serving.
///
/// Architecture (when fully implemented):
/// - Serves the local SystemSnapshot as JSON on a tiny HTTP server
/// - Registers via mDNS/Bonjour for automatic peer discovery
/// - Polls discovered peers for their snapshots
/// - Aggregates into FleetSnapshot for the UI
pub struct FleetManager {
    config: FleetConfig,
    peers: Vec<FleetPeer>,
}

impl FleetManager {
    pub fn new(config: FleetConfig) -> Self {
        Self {
            config,
            peers: Vec::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn snapshot(&self) -> FleetSnapshot {
        FleetSnapshot {
            peers: self.peers.clone(),
        }
    }

    /// Publish the local snapshot for peers to fetch.
    /// Placeholder — requires HTTP server implementation (tiny_http).
    pub fn publish_snapshot(&mut self, _snapshot: &SystemSnapshot) {
        // TODO: serve via HTTP
    }

    /// Discover and poll peers on the local network.
    /// Placeholder — requires mDNS/Bonjour integration.
    pub fn refresh_peers(&mut self) {
        // TODO: mDNS discovery + HTTP polling
    }
}
