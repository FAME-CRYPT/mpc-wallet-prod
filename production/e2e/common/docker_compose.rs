use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{info, warn};

/// Docker Compose orchestration wrapper
pub struct DockerCompose {
    compose_file: PathBuf,
    project_name: String,
}

impl DockerCompose {
    pub fn new(compose_file: impl AsRef<Path>, project_name: impl Into<String>) -> Self {
        Self {
            compose_file: compose_file.as_ref().to_path_buf(),
            project_name: project_name.into(),
        }
    }

    /// Start all services defined in docker-compose file
    pub fn up(&self) -> Result<()> {
        info!("Starting Docker Compose services: {}", self.project_name);

        let output = Command::new("docker-compose")
            .args(&[
                "-f",
                self.compose_file.to_str().unwrap(),
                "-p",
                &self.project_name,
                "up",
                "-d",
                "--build",
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .context("Failed to execute docker-compose up")?;

        if !output.status.success() {
            anyhow::bail!("docker-compose up failed with status: {}", output.status);
        }

        info!("Docker Compose services started successfully");
        Ok(())
    }

    /// Stop all services
    pub fn down(&self) -> Result<()> {
        info!("Stopping Docker Compose services: {}", self.project_name);

        let output = Command::new("docker-compose")
            .args(&[
                "-f",
                self.compose_file.to_str().unwrap(),
                "-p",
                &self.project_name,
                "down",
                "-v",
                "--remove-orphans",
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .context("Failed to execute docker-compose down")?;

        if !output.status.success() {
            warn!("docker-compose down failed with status: {}", output.status);
        }

        info!("Docker Compose services stopped");
        Ok(())
    }

    /// Get logs from a specific service
    pub fn logs(&self, service: &str) -> Result<String> {
        let output = Command::new("docker-compose")
            .args(&[
                "-f",
                self.compose_file.to_str().unwrap(),
                "-p",
                &self.project_name,
                "logs",
                service,
            ])
            .output()
            .context("Failed to get docker-compose logs")?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Execute a command in a running service container
    pub fn exec(&self, service: &str, command: &[&str]) -> Result<String> {
        let mut args = vec![
            "-f",
            self.compose_file.to_str().unwrap(),
            "-p",
            &self.project_name,
            "exec",
            "-T",
            service,
        ];
        args.extend_from_slice(command);

        let output = Command::new("docker-compose")
            .args(&args)
            .output()
            .context("Failed to execute command in container")?;

        if !output.status.success() {
            anyhow::bail!(
                "Command failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Kill a specific service (simulate crash)
    pub fn kill(&self, service: &str) -> Result<()> {
        info!("Killing service: {}", service);

        let output = Command::new("docker-compose")
            .args(&[
                "-f",
                self.compose_file.to_str().unwrap(),
                "-p",
                &self.project_name,
                "kill",
                service,
            ])
            .output()
            .context("Failed to kill service")?;

        if !output.status.success() {
            anyhow::bail!("Failed to kill service: {}", service);
        }

        Ok(())
    }

    /// Restart a specific service
    pub fn restart(&self, service: &str) -> Result<()> {
        info!("Restarting service: {}", service);

        let output = Command::new("docker-compose")
            .args(&[
                "-f",
                self.compose_file.to_str().unwrap(),
                "-p",
                &self.project_name,
                "restart",
                service,
            ])
            .output()
            .context("Failed to restart service")?;

        if !output.status.success() {
            anyhow::bail!("Failed to restart service: {}", service);
        }

        Ok(())
    }

    /// Pause a service (simulate freeze)
    pub fn pause(&self, service: &str) -> Result<()> {
        info!("Pausing service: {}", service);

        let output = Command::new("docker")
            .args(&["pause", &format!("{}-{}-1", self.project_name, service)])
            .output()
            .context("Failed to pause service")?;

        if !output.status.success() {
            anyhow::bail!("Failed to pause service: {}", service);
        }

        Ok(())
    }

    /// Unpause a service
    pub fn unpause(&self, service: &str) -> Result<()> {
        info!("Unpausing service: {}", service);

        let output = Command::new("docker")
            .args(&["unpause", &format!("{}-{}-1", self.project_name, service)])
            .output()
            .context("Failed to unpause service")?;

        if !output.status.success() {
            anyhow::bail!("Failed to unpause service: {}", service);
        }

        Ok(())
    }

    /// Create network partition by disconnecting service from network
    pub fn disconnect_network(&self, service: &str, network: &str) -> Result<()> {
        info!("Disconnecting {} from network {}", service, network);

        let container_name = format!("{}-{}-1", self.project_name, service);
        let network_name = format!("{}_{}", self.project_name, network);

        let output = Command::new("docker")
            .args(&["network", "disconnect", &network_name, &container_name])
            .output()
            .context("Failed to disconnect network")?;

        if !output.status.success() {
            anyhow::bail!("Failed to disconnect network");
        }

        Ok(())
    }

    /// Reconnect service to network (heal partition)
    pub fn connect_network(&self, service: &str, network: &str) -> Result<()> {
        info!("Reconnecting {} to network {}", service, network);

        let container_name = format!("{}-{}-1", self.project_name, service);
        let network_name = format!("{}_{}", self.project_name, network);

        let output = Command::new("docker")
            .args(&["network", "connect", &network_name, &container_name])
            .output()
            .context("Failed to reconnect network")?;

        if !output.status.success() {
            anyhow::bail!("Failed to reconnect network");
        }

        Ok(())
    }
}
