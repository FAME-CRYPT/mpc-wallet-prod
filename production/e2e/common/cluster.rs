use super::{ApiClient, DockerCompose};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use tokio_postgres::{Client, NoTls};
use tracing::{error, info, warn};

/// MPC Wallet cluster handle for E2E tests
pub struct Cluster {
    pub docker: DockerCompose,
    pub postgres: PostgresClient,
    pub etcd: EtcdClient,
    pub nodes: Vec<ApiClient>,
    pub project_name: String,
}

impl Cluster {
    /// Start a new cluster with the given compose file
    pub async fn start(compose_file: PathBuf, test_name: &str) -> Result<Self> {
        let project_name = format!("e2e-{}", test_name);
        let docker = DockerCompose::new(&compose_file, &project_name);

        info!("Starting cluster for test: {}", test_name);

        // Start Docker Compose services
        docker.up().context("Failed to start Docker Compose")?;

        // Wait for services to be healthy
        Self::wait_for_health(&docker, test_name).await?;

        // Connect to PostgreSQL
        let postgres = PostgresClient::connect(
            "postgresql://mpc:mpcpassword@localhost:5432/mpc_wallet",
        )
        .await
        .context("Failed to connect to PostgreSQL")?;

        // Connect to etcd
        let etcd = EtcdClient::connect(vec!["http://127.0.0.1:2379".to_string()])
            .await
            .context("Failed to connect to etcd")?;

        // Create API clients for all nodes
        let nodes = vec![
            ApiClient::new("http://localhost:8081"),
            ApiClient::new("http://localhost:8082"),
            ApiClient::new("http://localhost:8083"),
            ApiClient::new("http://localhost:8084"),
            ApiClient::new("http://localhost:8085"),
        ];

        info!("Cluster started successfully for test: {}", test_name);

        Ok(Self {
            docker,
            postgres,
            etcd,
            nodes,
            project_name,
        })
    }

    /// Wait for all services to be healthy
    async fn wait_for_health(docker: &DockerCompose, test_name: &str) -> Result<()> {
        info!("Waiting for services to be healthy...");

        let max_wait = Duration::from_secs(120);
        let start = std::time::Instant::now();

        // Wait for etcd cluster
        for etcd_node in &["etcd-1", "etcd-2", "etcd-3"] {
            let mut healthy = false;
            while start.elapsed() < max_wait {
                match docker.exec(etcd_node, &["etcdctl", "endpoint", "health"]) {
                    Ok(_) => {
                        info!("{} is healthy", etcd_node);
                        healthy = true;
                        break;
                    }
                    Err(_) => {
                        sleep(Duration::from_secs(2)).await;
                    }
                }
            }
            if !healthy {
                anyhow::bail!("{} did not become healthy in time", etcd_node);
            }
        }

        // Wait for PostgreSQL
        let mut pg_healthy = false;
        while start.elapsed() < max_wait {
            match docker.exec("postgres", &["pg_isready", "-U", "mpc"]) {
                Ok(_) => {
                    info!("PostgreSQL is healthy");
                    pg_healthy = true;
                    break;
                }
                Err(_) => {
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
        if !pg_healthy {
            anyhow::bail!("PostgreSQL did not become healthy in time");
        }

        // Wait for MPC nodes
        let node_ports = vec![8081, 8082, 8083, 8084, 8085];
        for (idx, port) in node_ports.iter().enumerate() {
            let client = ApiClient::new(format!("http://localhost:{}", port));
            let mut healthy = false;

            while start.elapsed() < max_wait {
                match client.health_check().await {
                    Ok(response) if response.status == "healthy" => {
                        info!("Node {} is healthy", idx + 1);
                        healthy = true;
                        break;
                    }
                    _ => {
                        sleep(Duration::from_secs(2)).await;
                    }
                }
            }

            if !healthy {
                anyhow::bail!("Node {} did not become healthy in time", idx + 1);
            }
        }

        info!("All services are healthy");
        Ok(())
    }

    /// Stop and clean up the cluster
    pub async fn stop(self) -> Result<()> {
        info!("Stopping cluster: {}", self.project_name);

        if let Err(e) = self.postgres.close().await {
            warn!("Failed to close PostgreSQL connection: {}", e);
        }

        if let Err(e) = self.docker.down() {
            error!("Failed to stop Docker Compose: {}", e);
            return Err(e);
        }

        info!("Cluster stopped successfully");
        Ok(())
    }
}

/// PostgreSQL client wrapper for E2E tests
pub struct PostgresClient {
    client: Client,
}

impl PostgresClient {
    pub async fn connect(connection_string: &str) -> Result<Self> {
        let (client, connection) = tokio_postgres::connect(connection_string, NoTls)
            .await
            .context("Failed to connect to PostgreSQL")?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("PostgreSQL connection error: {}", e);
            }
        });

        Ok(Self { client })
    }

    pub async fn query_signatures(&self, txid: &str) -> Result<Vec<SignatureRecord>> {
        let rows = self
            .client
            .query(
                "SELECT node_id, signature FROM votes WHERE tx_id = $1",
                &[&txid],
            )
            .await
            .context("Failed to query signatures")?;

        Ok(rows
            .into_iter()
            .map(|row| SignatureRecord {
                node_id: row.get::<_, i64>(0) as u64,
                signature: row.get(1),
            })
            .collect())
    }

    pub async fn query_byzantine_violations(&self) -> Result<Vec<ByzantineViolationRecord>> {
        let rows = self
            .client
            .query(
                "SELECT id, node_id, violation_type, tx_id, evidence, detected_at
                 FROM byzantine_violations
                 ORDER BY detected_at DESC",
                &[],
            )
            .await
            .context("Failed to query Byzantine violations")?;

        Ok(rows
            .into_iter()
            .map(|row| ByzantineViolationRecord {
                id: row.get(0),
                node_id: row.get::<_, Option<i64>>(1).map(|n| n as u64),
                violation_type: row.get(2),
                tx_id: row.get(3),
                evidence: row.get(4),
                detected_at: row.get(5),
            })
            .collect())
    }

    pub async fn query_transaction_state(&self, txid: &str) -> Result<Option<String>> {
        let row = self
            .client
            .query_opt("SELECT state FROM transactions WHERE txid = $1", &[&txid])
            .await
            .context("Failed to query transaction state")?;

        Ok(row.map(|r| r.get(0)))
    }

    pub async fn query_audit_log(&self, tx_id: &str) -> Result<Vec<AuditLogRecord>> {
        let rows = self
            .client
            .query(
                "SELECT event_type, node_id, details, timestamp
                 FROM audit_log
                 WHERE tx_id = $1
                 ORDER BY timestamp",
                &[&tx_id],
            )
            .await
            .context("Failed to query audit log")?;

        Ok(rows
            .into_iter()
            .map(|row| AuditLogRecord {
                event_type: row.get(0),
                node_id: row.get::<_, Option<i64>>(1).map(|n| n as u64),
                details: row.get(2),
                timestamp: row.get(3),
            })
            .collect())
    }

    pub async fn close(self) -> Result<()> {
        // Client will be closed when dropped
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SignatureRecord {
    pub node_id: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ByzantineViolationRecord {
    pub id: i64,
    pub node_id: Option<u64>,
    pub violation_type: String,
    pub tx_id: Option<String>,
    pub evidence: serde_json::Value,
    pub detected_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct AuditLogRecord {
    pub event_type: String,
    pub node_id: Option<u64>,
    pub details: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// etcd client wrapper for E2E tests
pub struct EtcdClient {
    client: etcd_client::Client,
}

impl EtcdClient {
    pub async fn connect(endpoints: Vec<String>) -> Result<Self> {
        let client = etcd_client::Client::connect(endpoints, None)
            .await
            .context("Failed to connect to etcd")?;

        Ok(Self { client })
    }

    pub async fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
        let response = self
            .client
            .get(key, None)
            .await
            .context("Failed to get key from etcd")?;

        Ok(response.kvs().first().map(|kv| kv.value().to_vec()))
    }

    pub async fn put(&mut self, key: &str, value: Vec<u8>) -> Result<()> {
        self.client
            .put(key, value, None)
            .await
            .context("Failed to put key to etcd")?;
        Ok(())
    }

    pub async fn is_node_banned(&mut self, node_id: u64) -> Result<bool> {
        let key = format!("/mpc/banned_nodes/{}", node_id);
        Ok(self.get(&key).await?.is_some())
    }

    pub async fn get_transaction_state(&mut self, txid: &str) -> Result<Option<String>> {
        let key = format!("/mpc/transactions/{}/state", txid);
        let value = self.get(&key).await?;

        Ok(value.map(|v| String::from_utf8_lossy(&v).to_string()))
    }

    pub async fn get_vote_count(&mut self, txid: &str, value: u64) -> Result<u64> {
        let key = format!("/mpc/votes/{}/{}", txid, value);
        let count_bytes = self.get(&key).await?;

        Ok(count_bytes
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }
}
