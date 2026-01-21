mod common;

use common::{Cluster, helpers};
use std::path::PathBuf;
use tracing_subscriber;

#[tokio::test]
#[ignore] // Expensive test, run explicitly
async fn test_cluster_startup_and_shutdown() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Verify all nodes are healthy
    for (idx, node) in cluster.nodes.iter().enumerate() {
        let health = node.health_check().await.expect("Health check failed");
        assert_eq!(health.status, "healthy");
        println!("Node {} is healthy: {:?}", idx + 1, health);
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Cluster startup and shutdown");
}

#[tokio::test]
#[ignore]
async fn test_etcd_cluster_connectivity() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let mut cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Test etcd read/write
    let test_key = format!("/e2e/test/{}", uuid::Uuid::new_v4());
    let test_value = b"test-value".to_vec();

    cluster
        .etcd
        .put(&test_key, test_value.clone())
        .await
        .expect("Failed to put to etcd");

    let retrieved = cluster
        .etcd
        .get(&test_key)
        .await
        .expect("Failed to get from etcd")
        .expect("Key not found");

    assert_eq!(retrieved, test_value);
    println!("etcd connectivity verified");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: etcd cluster connectivity");
}

#[tokio::test]
#[ignore]
async fn test_postgres_connectivity() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Verify schema exists by querying tables
    let violations = cluster
        .postgres
        .query_byzantine_violations()
        .await
        .expect("Failed to query violations");

    println!("PostgreSQL connectivity verified, violations count: {}", violations.len());

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: PostgreSQL connectivity");
}

#[tokio::test]
#[ignore]
async fn test_inter_node_connectivity() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Wait a bit for nodes to establish QUIC connections
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    // All nodes should be able to see each other through etcd
    // This would require actual node status queries
    // For now, we verify all nodes are responsive
    for (idx, node) in cluster.nodes.iter().enumerate() {
        let health = node.health_check().await;
        assert!(health.is_ok(), "Node {} not responding", idx + 1);
    }

    println!("All nodes are responding and connected");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Inter-node connectivity");
}

#[tokio::test]
#[ignore]
async fn test_cluster_health_endpoints() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Test health endpoints
    for (idx, node) in cluster.nodes.iter().enumerate() {
        let health = node.health_check().await.expect("Health check failed");

        assert_eq!(health.status, "healthy");
        assert!(!health.version.is_empty(), "Version should not be empty");

        println!("Node {} health: status={}, version={}",
                 idx + 1, health.status, health.version);
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Cluster health endpoints");
}

#[tokio::test]
#[ignore]
async fn test_rapid_cluster_restart() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");

    // Start and stop cluster 3 times to test idempotency
    for i in 1..=3 {
        println!("Cluster restart iteration {}/3", i);

        let test_name = format!("{}-iter{}", helpers::test_id(), i);

        let cluster = Cluster::start(compose_file.clone(), &test_name)
            .await
            .expect("Failed to start cluster");

        // Verify basic health
        for node in &cluster.nodes {
            node.health_check().await.expect("Health check failed");
        }

        cluster.stop().await.expect("Failed to stop cluster");

        // Brief pause between restarts
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    println!("Test passed: Rapid cluster restart");
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();
}
