// HTTP client for communicating with the MessageBoard service
// Provides methods to post and retrieve messages for the threshold signing protocol

use anyhow::Result;
use log::{debug, error};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Client for interacting with the MessageBoard HTTP API
#[derive(Clone)]
pub struct MessageBoardClient {
    pub(crate) base_url: String,
    pub(crate) client: Client,
    pub(crate) node_id: String,
}

/// Represents a signing request from the MessageBoard
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SigningRequest {
    pub id: String,
    pub message: String,
    pub status: String,
}

/// Represents a protocol message between nodes
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub request_id: String,
    pub from_node: String,
    pub to_node: String,
    pub round: i32,
    pub payload: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>, // ISO 8601 timestamp from Go
}

/// Response from posting a message
#[derive(Debug, Deserialize)]
struct PostMessageResponse {
    message_id: String,
}

/// Response from getting messages
#[derive(Debug, Deserialize)]
struct GetMessagesResponse {
    #[serde(default)]
    messages: Vec<NodeMessage>,
}

impl MessageBoardClient {
    /// Create a new MessageBoard client
    pub fn new(base_url: String, node_id: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
            node_id,
        }
    }

    /// Post a message to the MessageBoard
    /// The message will be sent from this node to the specified recipient
    pub async fn post_message(
        &self,
        request_id: &str,
        to_node: &str,
        round: i32,
        payload: &str,
    ) -> Result<String> {
        let url = format!("{}/messages", self.base_url);

        let msg = NodeMessage {
            id: None,
            request_id: request_id.to_string(),
            from_node: self.node_id.clone(),
            to_node: to_node.to_string(),
            round,
            payload: payload.to_string(),
            created_at: None,
        };

        debug!(
            "Posting message: {} -> {} (round {})",
            self.node_id, to_node, round
        );

        let response = self.client.post(&url).json(&msg).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Failed to post message: {} - {}", status, body);
            anyhow::bail!("Failed to post message: {}", status);
        }

        let result: PostMessageResponse = response.json().await?;
        Ok(result.message_id)
    }

    /// Get messages for a specific request addressed to this node
    /// Returns all messages where to_node equals this node's ID or "*" (broadcast)
    pub async fn get_messages(&self, request_id: &str) -> Result<Vec<NodeMessage>> {
        let url = format!(
            "{}/messages?request_id={}&to_node={}",
            self.base_url, request_id, self.node_id
        );

        debug!("Getting messages for request: {}", request_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            error!("Failed to get messages: {}", status);
            anyhow::bail!("Failed to get messages: {}", status);
        }

        let result: GetMessagesResponse = response.json().await?;
        Ok(result.messages)
    }

    /// Get a specific signing request by ID
    pub async fn get_request(&self, request_id: &str) -> Result<SigningRequest> {
        let url = format!("{}/requests/{}", self.base_url, request_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            anyhow::bail!("Failed to get request: {}", status);
        }

        let request: SigningRequest = response.json().await?;
        Ok(request)
    }

    /// Submit a completed signature for a request
    pub async fn submit_signature(&self, request_id: &str, signature: &str) -> Result<()> {
        let url = format!("{}/requests/{}", self.base_url, request_id);

        let body = serde_json::json!({
            "signature": signature
        });

        debug!("Submitting signature for request: {}", request_id);

        let response = self.client.put(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            error!("Failed to submit signature: {} - {}", status, body_text);
            anyhow::bail!("Failed to submit signature: {}", status);
        }

        debug!("Signature submitted successfully");
        Ok(())
    }

    /// Update the status of a signing request
    pub async fn update_status(&self, request_id: &str, status: &str) -> Result<()> {
        let url = format!("{}/requests/{}", self.base_url, request_id);

        let body = serde_json::json!({
            "status": status
        });

        let response = self.client.put(&url).json(&body).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to update status");
        }

        Ok(())
    }

    // ========== Presignature-specific methods ==========

    /// Post a presignature message to the MessageBoard (uses separate endpoint)
    pub async fn post_presignature_message(
        &self,
        request_id: &str,
        to_node: &str,
        round: i32,
        payload: &str,
    ) -> Result<String> {
        let url = format!("{}/presignature-messages", self.base_url);

        let msg = NodeMessage {
            id: None,
            request_id: request_id.to_string(),
            from_node: self.node_id.clone(),
            to_node: to_node.to_string(),
            round,
            payload: payload.to_string(),
            created_at: None,
        };

        debug!(
            "Posting presignature message: {} -> {} (round {})",
            self.node_id, to_node, round
        );

        let response = self.client.post(&url).json(&msg).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Failed to post presignature message: {} - {}", status, body);
            anyhow::bail!("Failed to post presignature message: {}", status);
        }

        let result: PostMessageResponse = response.json().await?;
        Ok(result.message_id)
    }

    /// Get presignature messages for a specific request addressed to this node
    pub async fn get_presignature_messages(&self, request_id: &str) -> Result<Vec<NodeMessage>> {
        let url = format!(
            "{}/presignature-messages?request_id={}&to_node={}",
            self.base_url, request_id, self.node_id
        );

        debug!("Getting presignature messages for request: {}", request_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            error!("Failed to get presignature messages: {}", status);
            anyhow::bail!("Failed to get presignature messages: {}", status);
        }

        let result: GetMessagesResponse = response.json().await?;
        Ok(result.messages)
    }

    /// Update the status of a presignature request
    pub async fn update_presignature_status(&self, request_id: &str, status: &str) -> Result<()> {
        let url = format!("{}/presignature-requests/{}", self.base_url, request_id);

        let body = serde_json::json!({
            "status": status
        });

        let response = self.client.put(&url).json(&body).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to update presignature status");
        }

        Ok(())
    }
}
