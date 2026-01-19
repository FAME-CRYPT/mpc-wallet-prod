// Presignature pool for managing pre-generated signatures
//
// Maintains a pool of ready-to-use presignatures for fast signing
// Automatically replenishes the pool in the background

use crate::presignature::StoredPresignature;
use anyhow::Result;
use log::info;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Pool of presignatures ready to use for signing
pub struct PresignaturePool<E: generic_ec::Curve, L: cggmp24::security_level::SecurityLevel> {
    /// Queue of available presignatures
    presignatures: Arc<Mutex<VecDeque<StoredPresignature<E, L>>>>,
    /// Target pool size (how many presignatures to keep ready)
    target_size: usize,
    /// Maximum pool size
    max_size: usize,
}

impl<E, L> PresignaturePool<E, L>
where
    E: generic_ec::Curve,
    L: cggmp24::security_level::SecurityLevel,
{
    /// Create a new presignature pool
    pub fn new(target_size: usize, max_size: usize) -> Self {
        Self {
            presignatures: Arc::new(Mutex::new(VecDeque::new())),
            target_size,
            max_size,
        }
    }

    /// Get the current pool size
    pub async fn size(&self) -> usize {
        self.presignatures.lock().await.len()
    }

    /// Check if the pool needs more presignatures
    pub async fn needs_refill(&self) -> bool {
        self.size().await < self.target_size
    }

    /// Add a presignature to the pool
    pub async fn add(&self, presignature: StoredPresignature<E, L>) -> Result<()> {
        let mut pool = self.presignatures.lock().await;

        if pool.len() >= self.max_size {
            info!(
                "Presignature pool is full ({}), discarding new presignature",
                self.max_size
            );
            return Ok(());
        }

        pool.push_back(presignature);
        info!(
            "Added presignature to pool, pool size: {}/{}",
            pool.len(),
            self.target_size
        );
        Ok(())
    }

    /// Take a presignature from the pool
    /// Returns None if pool is empty
    pub async fn take(&self) -> Option<StoredPresignature<E, L>> {
        let mut pool = self.presignatures.lock().await;
        let presig = pool.pop_front();

        if let Some(_) = &presig {
            info!(
                "Took presignature from pool, remaining: {}/{}",
                pool.len(),
                self.target_size
            );
        } else {
            info!("Presignature pool is empty!");
        }

        presig
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let size = self.size().await;
        PoolStats {
            current_size: size,
            target_size: self.target_size,
            max_size: self.max_size,
            utilization: (size as f64 / self.target_size as f64 * 100.0) as u32,
        }
    }

    /// Get a clone of the pool handle for sharing across tasks
    pub fn clone_handle(&self) -> PresignaturePoolHandle<E, L> {
        PresignaturePoolHandle {
            presignatures: Arc::clone(&self.presignatures),
            target_size: self.target_size,
            max_size: self.max_size,
        }
    }
}

/// A handle to the presignature pool that can be cloned and shared
#[derive(Clone)]
pub struct PresignaturePoolHandle<E: generic_ec::Curve, L: cggmp24::security_level::SecurityLevel> {
    presignatures: Arc<Mutex<VecDeque<StoredPresignature<E, L>>>>,
    target_size: usize,
    max_size: usize,
}

impl<E, L> PresignaturePoolHandle<E, L>
where
    E: generic_ec::Curve,
    L: cggmp24::security_level::SecurityLevel,
{
    /// Get the current pool size
    pub async fn size(&self) -> usize {
        self.presignatures.lock().await.len()
    }

    /// Check if the pool needs more presignatures
    pub async fn needs_refill(&self) -> bool {
        self.size().await < self.target_size
    }

    /// Add a presignature to the pool
    pub async fn add(&self, presignature: StoredPresignature<E, L>) -> Result<()> {
        let mut pool = self.presignatures.lock().await;

        if pool.len() >= self.max_size {
            info!(
                "Presignature pool is full ({}), discarding new presignature",
                self.max_size
            );
            return Ok(());
        }

        pool.push_back(presignature);
        info!(
            "Added presignature to pool, pool size: {}/{}",
            pool.len(),
            self.target_size
        );
        Ok(())
    }

    /// Take a presignature from the pool
    pub async fn take(&self) -> Option<StoredPresignature<E, L>> {
        let mut pool = self.presignatures.lock().await;
        let presig = pool.pop_front();

        if let Some(_) = &presig {
            info!(
                "Took presignature from pool, remaining: {}/{}",
                pool.len(),
                self.target_size
            );
        }

        presig
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let size = self.size().await;
        PoolStats {
            current_size: size,
            target_size: self.target_size,
            max_size: self.max_size,
            utilization: (size as f64 / self.target_size as f64 * 100.0) as u32,
        }
    }
}

/// Statistics about the presignature pool
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub current_size: usize,
    pub target_size: usize,
    pub max_size: usize,
    pub utilization: u32, // percentage
}
