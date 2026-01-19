use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use threshold_types::{NodeId, SystemConfig, VotingError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub node: NodeConfig,
    pub mtls: MtlsConfig,
    pub network: NetworkConfig,
    pub consensus: ConsensusConfig,
    pub etcd: EtcdConfig,
    pub postgres: PostgresConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_id: u64,
    pub listen_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtlsConfig {
    pub ca_cert_path: String,
    pub node_cert_path: String,
    pub node_key_path: String,
    pub tls_version: String, // "1.3"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub bootstrap_peers: Vec<String>,
    pub heartbeat_interval_secs: u64,
    pub reconnect_delay_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    pub total_nodes: usize,
    pub threshold: usize,
    pub vote_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtcdConfig {
    pub endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub url: String,
    pub max_connections: u32,
}

impl AppConfig {
    pub fn load() -> Result<Self, VotingError> {
        // Create default config structure
        let mut app_config = AppConfig {
            node: NodeConfig {
                node_id: 1,
                listen_addr: "0.0.0.0:9000".to_string(),
            },
            mtls: MtlsConfig {
                ca_cert_path: "certs/ca.crt".to_string(),
                node_cert_path: "certs/node1.crt".to_string(),
                node_key_path: "certs/node1.key".to_string(),
                tls_version: "1.3".to_string(),
            },
            network: NetworkConfig {
                bootstrap_peers: vec![],
                heartbeat_interval_secs: 30,
                reconnect_delay_secs: 5,
            },
            consensus: ConsensusConfig {
                total_nodes: 5,
                threshold: 4,
                vote_timeout_secs: 300,
            },
            etcd: EtcdConfig {
                endpoints: vec!["http://127.0.0.1:2379".to_string()],
            },
            postgres: PostgresConfig {
                url: "postgresql://mpc:mpc_password@localhost:5432/mpc_wallet".to_string(),
                max_connections: 10,
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
            config.node.node_id = node_id
                .parse()
                .map_err(|e| VotingError::ConfigError(format!("Invalid NODE_ID: {}", e)))?;
        }

        if let Ok(listen_addr) = std::env::var("LISTEN_ADDR") {
            config.node.listen_addr = listen_addr;
        }

        if let Ok(ca_cert_path) = std::env::var("CA_CERT_PATH") {
            config.mtls.ca_cert_path = ca_cert_path;
        }

        if let Ok(node_cert_path) = std::env::var("NODE_CERT_PATH") {
            config.mtls.node_cert_path = node_cert_path;
        }

        if let Ok(node_key_path) = std::env::var("NODE_KEY_PATH") {
            config.mtls.node_key_path = node_key_path;
        }

        if let Ok(etcd_endpoints) = std::env::var("ETCD_ENDPOINTS") {
            config.etcd.endpoints = etcd_endpoints
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }

        if let Ok(postgres_url) = std::env::var("POSTGRES_URL") {
            config.postgres.url = postgres_url;
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

        if self.node.node_id == 0 {
            return Err(VotingError::ConfigError("node_id cannot be 0".to_string()));
        }

        if self.etcd.endpoints.is_empty() {
            return Err(VotingError::ConfigError(
                "etcd endpoints cannot be empty".to_string(),
            ));
        }

        if self.postgres.url.is_empty() {
            return Err(VotingError::ConfigError(
                "postgres_url cannot be empty".to_string(),
            ));
        }

        // Validate TLS version
        if self.mtls.tls_version != "1.3" {
            return Err(VotingError::ConfigError(
                "Only TLS 1.3 is supported".to_string(),
            ));
        }

        Ok(())
    }

    pub fn node_id(&self) -> NodeId {
        NodeId(self.node.node_id)
    }

    pub fn listen_addr(&self) -> Result<SocketAddr, VotingError> {
        self.node
            .listen_addr
            .parse()
            .map_err(|e| VotingError::ConfigError(format!("Invalid listen_addr: {}", e)))
    }

    pub fn bootstrap_peers(&self) -> Result<Vec<SocketAddr>, VotingError> {
        self.network
            .bootstrap_peers
            .iter()
            .map(|addr| {
                addr.parse()
                    .map_err(|e| VotingError::ConfigError(format!("Invalid bootstrap peer {}: {}", addr, e)))
            })
            .collect()
    }
}
