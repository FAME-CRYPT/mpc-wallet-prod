//! Testcontainers setup for PostgreSQL and etcd.

use testcontainers::{clients::Cli, core::WaitFor, GenericImage, RunnableImage};
use std::sync::Arc;

/// PostgreSQL test container
pub struct PostgresContainer {
    _docker: Arc<Cli>,
    _container: testcontainers::Container<'static, GenericImage>,
    connection_string: String,
}

impl PostgresContainer {
    /// Create and start a new PostgreSQL container
    pub async fn new() -> Self {
        let docker = Arc::new(Cli::default());

        let image = GenericImage::new("postgres", "16-alpine")
            .with_env_var("POSTGRES_PASSWORD", "test")
            .with_env_var("POSTGRES_USER", "test")
            .with_env_var("POSTGRES_DB", "mpc_wallet_test")
            .with_wait_for(WaitFor::message_on_stderr("database system is ready to accept connections"));

        let runnable = RunnableImage::from(image)
            .with_tag("16-alpine");

        let container = docker.run(runnable);
        let port = container.get_host_port_ipv4(5432);

        let connection_string = format!(
            "postgresql://test:test@127.0.0.1:{}/mpc_wallet_test",
            port
        );

        // Wait for PostgreSQL to be ready
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Initialize schema
        Self::init_schema(&connection_string).await;

        Self {
            _docker: docker,
            _container: unsafe { std::mem::transmute(container) },
            connection_string,
        }
    }

    /// Get connection string
    pub fn connection_string(&self) -> String {
        self.connection_string.clone()
    }

    /// Initialize database schema
    async fn init_schema(connection_string: &str) {
        let (client, connection) = tokio_postgres::connect(connection_string, tokio_postgres::NoTls)
            .await
            .expect("Failed to connect to test database");

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("PostgreSQL connection error: {}", e);
            }
        });

        // Create tables
        let schema = r#"
            CREATE TABLE IF NOT EXISTS transactions (
                id SERIAL PRIMARY KEY,
                txid VARCHAR(255) UNIQUE NOT NULL,
                state VARCHAR(50) NOT NULL,
                unsigned_tx BYTEA NOT NULL,
                signed_tx BYTEA,
                recipient VARCHAR(255) NOT NULL,
                amount_sats BIGINT NOT NULL,
                fee_sats BIGINT NOT NULL,
                metadata TEXT,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS voting_rounds (
                id SERIAL PRIMARY KEY,
                tx_id VARCHAR(255) NOT NULL,
                round_number INTEGER NOT NULL,
                total_nodes INTEGER NOT NULL,
                threshold INTEGER NOT NULL,
                votes_received INTEGER DEFAULT 0,
                approved BOOLEAN DEFAULT FALSE,
                completed BOOLEAN DEFAULT FALSE,
                timeout_at TIMESTAMP WITH TIME ZONE,
                started_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                completed_at TIMESTAMP WITH TIME ZONE,
                UNIQUE(tx_id, round_number)
            );

            CREATE TABLE IF NOT EXISTS votes (
                id SERIAL PRIMARY KEY,
                round_id INTEGER NOT NULL REFERENCES voting_rounds(id),
                node_id BIGINT NOT NULL,
                tx_id VARCHAR(255) NOT NULL,
                approve BOOLEAN NOT NULL,
                value BIGINT NOT NULL,
                signature BYTEA NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                UNIQUE(round_id, node_id)
            );

            CREATE TABLE IF NOT EXISTS byzantine_violations (
                id SERIAL PRIMARY KEY,
                node_id BIGINT,
                violation_type VARCHAR(50) NOT NULL,
                round_id BIGINT NOT NULL,
                tx_id VARCHAR(255) NOT NULL,
                evidence JSONB NOT NULL,
                detected_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS node_status (
                node_id BIGINT PRIMARY KEY,
                status VARCHAR(50) NOT NULL,
                last_heartbeat TIMESTAMP WITH TIME ZONE NOT NULL,
                total_votes BIGINT DEFAULT 0,
                total_violations BIGINT DEFAULT 0,
                banned_until TIMESTAMP WITH TIME ZONE,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS presignature_usage (
                id SERIAL PRIMARY KEY,
                presig_id VARCHAR(255) UNIQUE NOT NULL,
                transaction_id BIGINT NOT NULL REFERENCES transactions(id),
                protocol VARCHAR(50) NOT NULL,
                generation_time_ms INTEGER NOT NULL,
                used_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS audit_log (
                id SERIAL PRIMARY KEY,
                event_type VARCHAR(100) NOT NULL,
                node_id BIGINT,
                tx_id VARCHAR(255),
                details JSONB NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            );
        "#;

        client
            .batch_execute(schema)
            .await
            .expect("Failed to create schema");
    }
}

/// etcd test container
pub struct EtcdContainer {
    _docker: Arc<Cli>,
    _container: testcontainers::Container<'static, GenericImage>,
    endpoints: Vec<String>,
}

impl EtcdContainer {
    /// Create and start a new etcd container
    pub async fn new() -> Self {
        let docker = Arc::new(Cli::default());

        let image = GenericImage::new("quay.io/coreos/etcd", "v3.5.11")
            .with_env_var("ETCD_LISTEN_CLIENT_URLS", "http://0.0.0.0:2379")
            .with_env_var("ETCD_ADVERTISE_CLIENT_URLS", "http://0.0.0.0:2379")
            .with_wait_for(WaitFor::message_on_stdout("ready to serve client requests"));

        let runnable = RunnableImage::from(image)
            .with_tag("v3.5.11");

        let container = docker.run(runnable);
        let port = container.get_host_port_ipv4(2379);

        let endpoints = vec![format!("127.0.0.1:{}", port)];

        // Wait for etcd to be ready
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        Self {
            _docker: docker,
            _container: unsafe { std::mem::transmute(container) },
            endpoints,
        }
    }

    /// Get etcd endpoints
    pub fn endpoints(&self) -> Vec<String> {
        self.endpoints.clone()
    }
}
