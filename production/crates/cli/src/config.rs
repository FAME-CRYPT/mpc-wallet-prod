//! Configuration management for threshold wallet CLI.
//!
//! Handles loading and saving configuration from ~/.threshold-wallet/config.toml

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// API server endpoint
    #[serde(default = "default_api_endpoint")]
    pub api_endpoint: String,

    /// Node ID for this CLI instance
    #[serde(default)]
    pub node_id: Option<u64>,

    /// Default timeout for API requests (seconds)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Output format (table, json)
    #[serde(default = "default_output_format")]
    pub output_format: String,

    /// Enable colored output
    #[serde(default = "default_colored")]
    pub colored: bool,
}

fn default_api_endpoint() -> String {
    "http://localhost:8080".to_string()
}

fn default_timeout() -> u64 {
    30
}

fn default_output_format() -> String {
    "table".to_string()
}

fn default_colored() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_endpoint: default_api_endpoint(),
            node_id: None,
            timeout_secs: default_timeout(),
            output_format: default_output_format(),
            colored: default_colored(),
        }
    }
}

impl Config {
    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .context("Could not determine home directory")?;

        Ok(home_dir.join(".threshold-wallet").join("config.toml"))
    }

    /// Get the path to the config directory
    pub fn config_dir() -> Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .context("Could not determine home directory")?;

        Ok(home_dir.join(".threshold-wallet"))
    }

    /// Load configuration from file, or create default if it doesn't exist
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)
                .context("Failed to read config file")?;

            let config: Config = toml::from_str(&contents)
                .context("Failed to parse config file")?;

            Ok(config)
        } else {
            // Create default config
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_dir = Self::config_dir()?;
        let config_path = Self::config_path()?;

        // Create config directory if it doesn't exist
        std::fs::create_dir_all(&config_dir)
            .context("Failed to create config directory")?;

        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        std::fs::write(&config_path, contents)
            .context("Failed to write config file")?;

        Ok(())
    }

    /// Update API endpoint
    pub fn set_api_endpoint(&mut self, endpoint: String) -> Result<()> {
        self.api_endpoint = endpoint;
        self.save()
    }

    /// Update node ID
    pub fn set_node_id(&mut self, node_id: u64) -> Result<()> {
        self.node_id = Some(node_id);
        self.save()
    }

    /// Update output format
    pub fn set_output_format(&mut self, format: String) -> Result<()> {
        if format != "table" && format != "json" {
            anyhow::bail!("Invalid output format. Must be 'table' or 'json'");
        }
        self.output_format = format;
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.api_endpoint, "http://localhost:8080");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.output_format, "table");
        assert!(config.colored);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.api_endpoint, deserialized.api_endpoint);
    }
}
