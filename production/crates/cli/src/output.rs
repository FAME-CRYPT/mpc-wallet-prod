//! Output formatting utilities for CLI.
//!
//! Provides table-based and JSON output modes with optional colorization.

use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use tabled::{settings::Style, Table, Tabled};

/// Output formatter
pub struct OutputFormatter {
    colored: bool,
    pub json_mode: bool,
}

impl OutputFormatter {
    /// Create a new output formatter
    pub fn new(colored: bool, json_mode: bool) -> Self {
        Self { colored, json_mode }
    }

    /// Print success message
    pub fn success(&self, message: &str) {
        if self.colored {
            println!("{} {}", "✓".green().bold(), message.green());
        } else {
            println!("✓ {}", message);
        }
    }

    /// Print error message
    pub fn error(&self, message: &str) {
        if self.colored {
            eprintln!("{} {}", "✗".red().bold(), message.red());
        } else {
            eprintln!("✗ {}", message);
        }
    }

    /// Print warning message
    pub fn warning(&self, message: &str) {
        if self.colored {
            println!("{} {}", "⚠".yellow().bold(), message.yellow());
        } else {
            println!("⚠ {}", message);
        }
    }

    /// Print info message
    pub fn info(&self, message: &str) {
        if self.colored {
            println!("{} {}", "ℹ".blue().bold(), message);
        } else {
            println!("ℹ {}", message);
        }
    }

    /// Print a header
    pub fn header(&self, title: &str) {
        if self.colored {
            println!("\n{}", title.bold().underline());
        } else {
            println!("\n{}", title);
        }
    }

    /// Print key-value pair
    pub fn kv(&self, key: &str, value: &str) {
        if self.colored {
            println!("  {}: {}", key.bold(), value);
        } else {
            println!("  {}: {}", key, value);
        }
    }

    /// Print as JSON
    pub fn json<T: Serialize>(&self, data: &T) -> Result<()> {
        let json = serde_json::to_string_pretty(data)?;
        println!("{}", json);
        Ok(())
    }

    /// Print a table
    pub fn table<T: Tabled>(&self, data: Vec<T>) {
        if data.is_empty() {
            self.info("No data to display");
            return;
        }

        let mut table = Table::new(data);
        table.with(Style::rounded());

        println!("\n{}", table);
    }

    /// Output data in the configured format (table or JSON)
    pub fn output<T: Serialize + Tabled>(&self, data: Vec<T>) -> Result<()> {
        if self.json_mode {
            self.json(&data)
        } else {
            self.table(data);
            Ok(())
        }
    }

    /// Format satoshis as BTC with 8 decimal places
    pub fn format_btc(&self, sats: u64) -> String {
        let btc = sats as f64 / 100_000_000.0;
        format!("{:.8} BTC", btc)
    }

    /// Format satoshis with thousands separator
    pub fn format_sats(&self, sats: u64) -> String {
        format!("{} sats", Self::format_number(sats))
    }

    /// Format number with thousands separator
    fn format_number(n: u64) -> String {
        let s = n.to_string();
        let mut result = String::new();
        for (i, c) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        result.chars().rev().collect()
    }

    /// Format transaction state with color
    pub fn format_state(&self, state: &str) -> String {
        if !self.colored {
            return state.to_string();
        }

        match state {
            "confirmed" => state.green().to_string(),
            "signed" | "submitted" | "broadcasting" => state.cyan().to_string(),
            "approved" | "threshold_reached" => state.blue().to_string(),
            "pending" | "voting" | "collecting" => state.yellow().to_string(),
            "signing" => state.magenta().to_string(),
            "failed" | "rejected" | "aborted_byzantine" => state.red().to_string(),
            _ => state.to_string(),
        }
    }

    /// Format node status with color
    pub fn format_node_status(&self, status: &str) -> String {
        if !self.colored {
            return status.to_string();
        }

        match status {
            "active" => status.green().to_string(),
            "inactive" => status.yellow().to_string(),
            "banned" => status.red().to_string(),
            _ => status.to_string(),
        }
    }

    /// Format boolean with color
    pub fn format_bool(&self, value: bool) -> String {
        if !self.colored {
            return value.to_string();
        }

        if value {
            "true".green().to_string()
        } else {
            "false".red().to_string()
        }
    }

    /// Format timestamp in human-readable form
    pub fn format_timestamp(&self, timestamp: &chrono::DateTime<chrono::Utc>) -> String {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(*timestamp);

        if duration.num_seconds() < 60 {
            format!("{} seconds ago", duration.num_seconds())
        } else if duration.num_minutes() < 60 {
            format!("{} minutes ago", duration.num_minutes())
        } else if duration.num_hours() < 24 {
            format!("{} hours ago", duration.num_hours())
        } else {
            format!("{} days ago", duration.num_days())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_btc() {
        let formatter = OutputFormatter::new(false, false);
        assert_eq!(formatter.format_btc(100_000_000), "1.00000000 BTC");
        assert_eq!(formatter.format_btc(50_000_000), "0.50000000 BTC");
        assert_eq!(formatter.format_btc(1_000), "0.00001000 BTC");
    }

    #[test]
    fn test_format_sats() {
        let formatter = OutputFormatter::new(false, false);
        assert_eq!(formatter.format_sats(1000), "1,000 sats");
        assert_eq!(formatter.format_sats(1_000_000), "1,000,000 sats");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(OutputFormatter::format_number(1000), "1,000");
        assert_eq!(OutputFormatter::format_number(1_000_000), "1,000,000");
        assert_eq!(OutputFormatter::format_number(999), "999");
    }
}
