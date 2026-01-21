//! Cluster status and monitoring commands.

use anyhow::Result;
use serde::Serialize;
use tabled::Tabled;

use crate::{client::ApiClient, output::OutputFormatter};

/// Get cluster status
pub async fn get_status(client: &ApiClient, formatter: &OutputFormatter) -> Result<()> {
    formatter.info("Fetching cluster status...");

    let status = client.get_cluster_status().await?;

    if formatter.json_mode {
        formatter.json(&status)?;
    } else {
        formatter.header("Cluster Status");
        formatter.kv("Status", &status.status);
        formatter.kv("Total Nodes", &status.total_nodes.to_string());
        formatter.kv("Healthy Nodes", &status.healthy_nodes.to_string());
        formatter.kv("Threshold", &status.threshold.to_string());
        formatter.kv(
            "Checked At",
            &formatter.format_timestamp(&status.timestamp),
        );

        println!();
        if status.healthy_nodes >= status.threshold {
            formatter.success("Cluster has sufficient healthy nodes for consensus");
        } else {
            formatter.warning(&format!(
                "Cluster needs {} more healthy node(s) to reach threshold",
                status.threshold - status.healthy_nodes
            ));
        }
    }

    Ok(())
}

/// List cluster nodes
pub async fn list_nodes(client: &ApiClient, formatter: &OutputFormatter) -> Result<()> {
    formatter.info("Fetching cluster nodes...");

    let response = client.list_cluster_nodes().await?;

    if formatter.json_mode {
        formatter.json(&response.nodes)?;
    } else {
        if response.total == 0 {
            formatter.info("No nodes found");
            return Ok(());
        }

        formatter.header(&format!("Cluster Nodes ({})", response.total));

        // Create table data
        let table_data: Vec<NodeTableRow> = response
            .nodes
            .into_iter()
            .map(|node| NodeTableRow {
                node_id: format!("node-{}", node.node_id),
                status: node.status,
                votes: node.total_votes.to_string(),
                violations: node.total_violations.to_string(),
                last_heartbeat: format_seconds_ago(node.seconds_since_heartbeat),
                banned: node.is_banned.to_string(),
            })
            .collect();

        formatter.table(table_data);
        println!();
        formatter.info(&format!("Total: {} node(s)", response.total));
    }

    Ok(())
}

/// Table row for node list
#[derive(Tabled, Serialize)]
struct NodeTableRow {
    #[tabled(rename = "Node ID")]
    node_id: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Votes")]
    votes: String,
    #[tabled(rename = "Violations")]
    violations: String,
    #[tabled(rename = "Last Heartbeat")]
    last_heartbeat: String,
    #[tabled(rename = "Banned")]
    banned: String,
}

/// Format seconds as human-readable duration
fn format_seconds_ago(seconds: f64) -> String {
    let secs = seconds as i64;
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}
