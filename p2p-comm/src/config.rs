use serde::{Deserialize, Serialize};
use std::path::Path;
use threshold_types::{NodeId, PeerId, SystemConfig, VotingError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub node: NodeConfig,
    pub network: NetworkConfig,
    pub storage: StorageConfig,
    pub consensus: ConsensusConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_id: String,
    pub peer_id: String,
    pub listen_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub bootstrap_peers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub etcd_endpoints: Vec<String>,
    pub postgres_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    pub total_nodes: usize,
    pub threshold: usize,
    pub vote_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

impl AppConfig {
    pub fn load() -> Result<Self, VotingError> {
        // Create default config structure
        let mut app_config = AppConfig {
            node: NodeConfig {
                node_id: "node_1".to_string(),
                peer_id: "peer_1".to_string(),
                listen_addr: "/ip4/0.0.0.0/tcp/9000".to_string(),
            },
            network: NetworkConfig {
                bootstrap_peers: vec![],
            },
            storage: StorageConfig {
                etcd_endpoints: vec!["http://127.0.0.1:2379".to_string()],
                postgres_url: "postgresql://threshold:threshold_pass@localhost:5432/threshold_voting".to_string(),
            },
            consensus: ConsensusConfig {
                total_nodes: 5,
                threshold: 4,
                vote_timeout_secs: 300,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
            },
        };

        // Try to load from file if exists
        if Path::new("config/default.toml").exists() {
            let settings = config::Config::builder()
                .add_source(config::File::with_name("config/default"))
                .build()
                .map_err(|e| VotingError::ConfigError(format!("Failed to load config file: {}", e)))?;

            if let Ok(file_config) = settings.try_deserialize::<AppConfig>() {
                app_config = file_config;
            }
        }

        // Override with environment variables
        Self::override_from_env(&mut app_config)?;

        Ok(app_config)
    }

    fn override_from_env(config: &mut AppConfig) -> Result<(), VotingError> {
        if let Ok(node_id) = std::env::var("NODE_ID") {
            config.node.node_id = node_id;
        }

        if let Ok(peer_id) = std::env::var("PEER_ID") {
            config.node.peer_id = peer_id;
        }

        if let Ok(listen_addr) = std::env::var("LISTEN_ADDR") {
            config.node.listen_addr = listen_addr;
        }

        if let Ok(etcd_endpoints) = std::env::var("ETCD_ENDPOINTS") {
            config.storage.etcd_endpoints = etcd_endpoints
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }

        if let Ok(postgres_url) = std::env::var("POSTGRES_URL") {
            config.storage.postgres_url = postgres_url;
        }

        if let Ok(total_nodes) = std::env::var("TOTAL_NODES") {
            config.consensus.total_nodes = total_nodes
                .parse()
                .map_err(|e| VotingError::ConfigError(format!("Invalid TOTAL_NODES: {}", e)))?;
        }

        if let Ok(threshold) = std::env::var("THRESHOLD") {
            config.consensus.threshold = threshold
                .parse()
                .map_err(|e| VotingError::ConfigError(format!("Invalid THRESHOLD: {}", e)))?;
        }

        if let Ok(timeout) = std::env::var("VOTE_TIMEOUT_SECS") {
            config.consensus.vote_timeout_secs = timeout
                .parse()
                .map_err(|e| VotingError::ConfigError(format!("Invalid VOTE_TIMEOUT_SECS: {}", e)))?;
        }

        if let Ok(bootstrap_peers) = std::env::var("BOOTSTRAP_PEERS") {
            if !bootstrap_peers.is_empty() {
                config.network.bootstrap_peers = bootstrap_peers
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }

        Ok(())
    }

    pub fn validate(&self) -> Result<(), VotingError> {
        SystemConfig::new(
            self.consensus.total_nodes,
            self.consensus.threshold,
            self.consensus.vote_timeout_secs,
        )?;

        if self.node.node_id.is_empty() {
            return Err(VotingError::ConfigError("node_id cannot be empty".to_string()));
        }

        if self.node.peer_id.is_empty() {
            return Err(VotingError::ConfigError("peer_id cannot be empty".to_string()));
        }

        if self.storage.etcd_endpoints.is_empty() {
            return Err(VotingError::ConfigError("etcd_endpoints cannot be empty".to_string()));
        }

        if self.storage.postgres_url.is_empty() {
            return Err(VotingError::ConfigError("postgres_url cannot be empty".to_string()));
        }

        Ok(())
    }

    pub fn node_id(&self) -> NodeId {
        NodeId::from(self.node.node_id.clone())
    }

    pub fn peer_id(&self) -> PeerId {
        PeerId::from(self.node.peer_id.clone())
    }
}
