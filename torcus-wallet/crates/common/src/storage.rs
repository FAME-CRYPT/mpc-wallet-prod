//! Persistent storage for MPC wallet data.
//!
//! Uses SQLite for durable storage of:
//! - Wallet metadata (coordinator)
//! - Key shares (MPC nodes)

use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

use crate::{MpcWalletError, WalletType};

// ============================================================================
// Wallet Storage (Coordinator)
// ============================================================================

/// Stored wallet information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredWallet {
    pub id: Uuid,
    pub name: String,
    pub wallet_type: WalletType,
    pub public_key: String,
    pub address: String,
    pub created_at: DateTime<Utc>,
}

/// Coordinator's wallet storage.
pub struct WalletStore {
    conn: Mutex<Connection>,
}

impl WalletStore {
    /// Open or create a wallet store at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, MpcWalletError> {
        let conn = Connection::open(path)
            .map_err(|e| MpcWalletError::Storage(format!("Failed to open database: {}", e)))?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;

        Ok(store)
    }

    /// Open an in-memory store (for testing).
    pub fn open_in_memory() -> Result<Self, MpcWalletError> {
        let conn = Connection::open_in_memory().map_err(|e| {
            MpcWalletError::Storage(format!("Failed to open in-memory database: {}", e))
        })?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;

        Ok(store)
    }

    /// Initialize database schema.
    fn init_schema(&self) -> Result<(), MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS wallets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                wallet_type TEXT NOT NULL,
                public_key TEXT NOT NULL,
                address TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| MpcWalletError::Storage(format!("Failed to create schema: {}", e)))?;

        tracing::debug!("Wallet store schema initialized");
        Ok(())
    }

    /// Save a wallet.
    pub fn save_wallet(&self, wallet: &StoredWallet) -> Result<(), MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        conn.execute(
            "INSERT OR REPLACE INTO wallets (id, name, wallet_type, public_key, address, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                wallet.id.to_string(),
                wallet.name,
                wallet.wallet_type.to_string(),
                wallet.public_key,
                wallet.address,
                wallet.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| MpcWalletError::Storage(format!("Failed to save wallet: {}", e)))?;

        tracing::info!("Saved wallet {} to storage", wallet.id);
        Ok(())
    }

    /// Get a wallet by ID.
    pub fn get_wallet(&self, id: Uuid) -> Result<Option<StoredWallet>, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let mut stmt = conn
            .prepare("SELECT id, name, wallet_type, public_key, address, created_at FROM wallets WHERE id = ?1")
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        let wallet = stmt
            .query_row(params![id.to_string()], |row| {
                let id_str: String = row.get(0)?;
                let id = Uuid::parse_str(&id_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        format!("invalid UUID '{}': {}", id_str, e).into(),
                    )
                })?;

                let created_at_str: String = row.get(5)?;
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            format!("invalid timestamp '{}': {}", created_at_str, e).into(),
                        )
                    })?
                    .with_timezone(&Utc);

                let wallet_type_str: String = row.get(2)?;
                let wallet_type = parse_wallet_type(&wallet_type_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        e.into(),
                    )
                })?;

                Ok(StoredWallet {
                    id,
                    name: row.get(1)?,
                    wallet_type,
                    public_key: row.get(3)?,
                    address: row.get(4)?,
                    created_at,
                })
            })
            .optional()
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        Ok(wallet)
    }

    /// List all wallets.
    pub fn list_wallets(&self) -> Result<Vec<StoredWallet>, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let mut stmt = conn
            .prepare("SELECT id, name, wallet_type, public_key, address, created_at FROM wallets ORDER BY created_at DESC")
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        let wallets = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let id = Uuid::parse_str(&id_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        format!("invalid UUID '{}': {}", id_str, e).into(),
                    )
                })?;

                let created_at_str: String = row.get(5)?;
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            format!("invalid timestamp '{}': {}", created_at_str, e).into(),
                        )
                    })?
                    .with_timezone(&Utc);

                let wallet_type_str: String = row.get(2)?;
                let wallet_type = parse_wallet_type(&wallet_type_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        e.into(),
                    )
                })?;

                Ok(StoredWallet {
                    id,
                    name: row.get(1)?,
                    wallet_type,
                    public_key: row.get(3)?,
                    address: row.get(4)?,
                    created_at,
                })
            })
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        Ok(wallets)
    }

    /// Delete a wallet.
    pub fn delete_wallet(&self, id: Uuid) -> Result<bool, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let rows = conn
            .execute("DELETE FROM wallets WHERE id = ?1", params![id.to_string()])
            .map_err(|e| MpcWalletError::Storage(format!("Delete error: {}", e)))?;

        Ok(rows > 0)
    }

    /// Get wallet count.
    pub fn wallet_count(&self) -> Result<usize, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wallets", [], |row| row.get(0))
            .map_err(|e| MpcWalletError::Storage(format!("Count error: {}", e)))?;

        Ok(count as usize)
    }
}

// ============================================================================
// Key Share Storage (MPC Nodes)
// ============================================================================

/// Stored key share for an MPC node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredKeyShare {
    pub wallet_id: Uuid,
    pub party_index: u16,
    /// Secret share (encrypted or raw bytes, hex encoded).
    pub secret_share: String,
    /// Public key for this wallet.
    pub public_key: String,
    /// All public key shares (JSON array of hex strings).
    pub public_key_shares: String,
    pub created_at: DateTime<Utc>,
}

/// MPC Node's key share storage.
pub struct KeyShareStore {
    conn: Mutex<Connection>,
    party_index: u16,
}

impl KeyShareStore {
    /// Open or create a key share store at the given path.
    pub fn open<P: AsRef<Path>>(path: P, party_index: u16) -> Result<Self, MpcWalletError> {
        let conn = Connection::open(path)
            .map_err(|e| MpcWalletError::Storage(format!("Failed to open database: {}", e)))?;

        let store = Self {
            conn: Mutex::new(conn),
            party_index,
        };
        store.init_schema()?;

        Ok(store)
    }

    /// Open an in-memory store (for testing).
    pub fn open_in_memory(party_index: u16) -> Result<Self, MpcWalletError> {
        let conn = Connection::open_in_memory().map_err(|e| {
            MpcWalletError::Storage(format!("Failed to open in-memory database: {}", e))
        })?;

        let store = Self {
            conn: Mutex::new(conn),
            party_index,
        };
        store.init_schema()?;

        Ok(store)
    }

    /// Initialize database schema.
    fn init_schema(&self) -> Result<(), MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS key_shares (
                wallet_id TEXT PRIMARY KEY,
                party_index INTEGER NOT NULL,
                secret_share TEXT NOT NULL,
                public_key TEXT NOT NULL,
                public_key_shares TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| MpcWalletError::Storage(format!("Failed to create schema: {}", e)))?;

        tracing::debug!(
            "Key share store schema initialized for party {}",
            self.party_index
        );
        Ok(())
    }

    /// Save a key share.
    pub fn save_key_share(&self, share: &StoredKeyShare) -> Result<(), MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        conn.execute(
            "INSERT OR REPLACE INTO key_shares
             (wallet_id, party_index, secret_share, public_key, public_key_shares, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                share.wallet_id.to_string(),
                share.party_index,
                share.secret_share,
                share.public_key,
                share.public_key_shares,
                share.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| MpcWalletError::Storage(format!("Failed to save key share: {}", e)))?;

        tracing::info!(
            "Party {} saved key share for wallet {}",
            self.party_index,
            share.wallet_id
        );
        Ok(())
    }

    /// Get a key share by wallet ID.
    pub fn get_key_share(&self, wallet_id: Uuid) -> Result<Option<StoredKeyShare>, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT wallet_id, party_index, secret_share, public_key, public_key_shares, created_at
                 FROM key_shares WHERE wallet_id = ?1",
            )
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        let share = stmt
            .query_row(params![wallet_id.to_string()], |row| {
                let wallet_id_str: String = row.get(0)?;
                let wallet_id = Uuid::parse_str(&wallet_id_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        format!("invalid UUID '{}': {}", wallet_id_str, e).into(),
                    )
                })?;

                let created_at_str: String = row.get(5)?;
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            format!("invalid timestamp '{}': {}", created_at_str, e).into(),
                        )
                    })?
                    .with_timezone(&Utc);

                Ok(StoredKeyShare {
                    wallet_id,
                    party_index: row.get::<_, i64>(1)? as u16,
                    secret_share: row.get(2)?,
                    public_key: row.get(3)?,
                    public_key_shares: row.get(4)?,
                    created_at,
                })
            })
            .optional()
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        Ok(share)
    }

    /// List all key shares.
    pub fn list_key_shares(&self) -> Result<Vec<StoredKeyShare>, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT wallet_id, party_index, secret_share, public_key, public_key_shares, created_at
                 FROM key_shares ORDER BY created_at DESC",
            )
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        let shares = stmt
            .query_map([], |row| {
                let wallet_id_str: String = row.get(0)?;
                let wallet_id = Uuid::parse_str(&wallet_id_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        format!("invalid UUID '{}': {}", wallet_id_str, e).into(),
                    )
                })?;

                let created_at_str: String = row.get(5)?;
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            format!("invalid timestamp '{}': {}", created_at_str, e).into(),
                        )
                    })?
                    .with_timezone(&Utc);

                Ok(StoredKeyShare {
                    wallet_id,
                    party_index: row.get::<_, i64>(1)? as u16,
                    secret_share: row.get(2)?,
                    public_key: row.get(3)?,
                    public_key_shares: row.get(4)?,
                    created_at,
                })
            })
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        Ok(shares)
    }

    /// Delete a key share.
    pub fn delete_key_share(&self, wallet_id: Uuid) -> Result<bool, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let rows = conn
            .execute(
                "DELETE FROM key_shares WHERE wallet_id = ?1",
                params![wallet_id.to_string()],
            )
            .map_err(|e| MpcWalletError::Storage(format!("Delete error: {}", e)))?;

        Ok(rows > 0)
    }

    /// Check if a key share exists.
    pub fn has_key_share(&self, wallet_id: Uuid) -> Result<bool, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM key_shares WHERE wallet_id = ?1",
                params![wallet_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        Ok(count > 0)
    }

    /// Get the party index.
    pub fn party_index(&self) -> u16 {
        self.party_index
    }
}

// ============================================================================
// Relay Session Storage (Coordinator)
// ============================================================================

/// Stored relay session for persistence across coordinator restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRelaySession {
    pub session_id: String,
    pub protocol: String,
    pub parties: Vec<u16>,
    pub started_at: u64,
    pub last_activity: u64,
    /// Serialized message queues (JSON: HashMap<u16, Vec<RelayMessage>>)
    pub message_queues_json: String,
    pub parties_ready: Vec<u16>,
    pub parties_completed: Vec<u16>,
    pub active: bool,
    pub error: Option<String>,
}

/// Relay session store for persisting sessions to SQLite.
pub struct RelaySessionStore {
    conn: Mutex<Connection>,
}

impl RelaySessionStore {
    /// Open or create a relay session store at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, MpcWalletError> {
        let conn = Connection::open(path)
            .map_err(|e| MpcWalletError::Storage(format!("Failed to open database: {}", e)))?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;

        Ok(store)
    }

    /// Open an in-memory store (for testing).
    pub fn open_in_memory() -> Result<Self, MpcWalletError> {
        let conn = Connection::open_in_memory().map_err(|e| {
            MpcWalletError::Storage(format!("Failed to open in-memory database: {}", e))
        })?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;

        Ok(store)
    }

    /// Initialize database schema.
    fn init_schema(&self) -> Result<(), MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS relay_sessions (
                session_id TEXT PRIMARY KEY,
                protocol TEXT NOT NULL,
                parties TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                last_activity INTEGER NOT NULL,
                message_queues_json TEXT NOT NULL,
                parties_ready TEXT NOT NULL,
                parties_completed TEXT NOT NULL,
                active INTEGER NOT NULL,
                error TEXT
            )",
            [],
        )
        .map_err(|e| MpcWalletError::Storage(format!("Failed to create schema: {}", e)))?;

        tracing::debug!("Relay session store schema initialized");
        Ok(())
    }

    /// Save or update a relay session.
    pub fn save_session(&self, session: &StoredRelaySession) -> Result<(), MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let parties_json = serde_json::to_string(&session.parties)
            .map_err(|e| MpcWalletError::Storage(e.to_string()))?;
        let parties_ready_json = serde_json::to_string(&session.parties_ready)
            .map_err(|e| MpcWalletError::Storage(e.to_string()))?;
        let parties_completed_json = serde_json::to_string(&session.parties_completed)
            .map_err(|e| MpcWalletError::Storage(e.to_string()))?;

        conn.execute(
            "INSERT OR REPLACE INTO relay_sessions
             (session_id, protocol, parties, started_at, last_activity, message_queues_json,
              parties_ready, parties_completed, active, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                session.session_id,
                session.protocol,
                parties_json,
                session.started_at as i64,
                session.last_activity as i64,
                session.message_queues_json,
                parties_ready_json,
                parties_completed_json,
                session.active as i32,
                session.error,
            ],
        )
        .map_err(|e| MpcWalletError::Storage(format!("Failed to save session: {}", e)))?;

        tracing::debug!("Saved relay session {} to storage", session.session_id);
        Ok(())
    }

    /// Get a session by ID.
    pub fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Option<StoredRelaySession>, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT session_id, protocol, parties, started_at, last_activity,
                        message_queues_json, parties_ready, parties_completed, active, error
                 FROM relay_sessions WHERE session_id = ?1",
            )
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        let session = stmt
            .query_row(params![session_id], |row| {
                let parties_json: String = row.get(2)?;
                let parties_ready_json: String = row.get(6)?;
                let parties_completed_json: String = row.get(7)?;

                let parties: Vec<u16> = serde_json::from_str(&parties_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        format!("invalid parties JSON: {}", e).into(),
                    )
                })?;

                let parties_ready: Vec<u16> =
                    serde_json::from_str(&parties_ready_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            format!("invalid parties_ready JSON: {}", e).into(),
                        )
                    })?;

                let parties_completed: Vec<u16> = serde_json::from_str(&parties_completed_json)
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            7,
                            rusqlite::types::Type::Text,
                            format!("invalid parties_completed JSON: {}", e).into(),
                        )
                    })?;

                Ok(StoredRelaySession {
                    session_id: row.get(0)?,
                    protocol: row.get(1)?,
                    parties,
                    started_at: row.get::<_, i64>(3)? as u64,
                    last_activity: row.get::<_, i64>(4)? as u64,
                    message_queues_json: row.get(5)?,
                    parties_ready,
                    parties_completed,
                    active: row.get::<_, i32>(8)? != 0,
                    error: row.get(9)?,
                })
            })
            .optional()
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        Ok(session)
    }

    /// List all active (non-expired) sessions.
    pub fn list_active_sessions(
        &self,
        ttl_secs: u64,
    ) -> Result<Vec<StoredRelaySession>, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cutoff = now.saturating_sub(ttl_secs) as i64;

        let mut stmt = conn
            .prepare(
                "SELECT session_id, protocol, parties, started_at, last_activity,
                        message_queues_json, parties_ready, parties_completed, active, error
                 FROM relay_sessions
                 WHERE last_activity > ?1
                 ORDER BY started_at DESC",
            )
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        let sessions = stmt
            .query_map(params![cutoff], |row| {
                let parties_json: String = row.get(2)?;
                let parties_ready_json: String = row.get(6)?;
                let parties_completed_json: String = row.get(7)?;

                let parties: Vec<u16> = serde_json::from_str(&parties_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        format!("invalid parties JSON: {}", e).into(),
                    )
                })?;

                let parties_ready: Vec<u16> =
                    serde_json::from_str(&parties_ready_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            format!("invalid parties_ready JSON: {}", e).into(),
                        )
                    })?;

                let parties_completed: Vec<u16> = serde_json::from_str(&parties_completed_json)
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            7,
                            rusqlite::types::Type::Text,
                            format!("invalid parties_completed JSON: {}", e).into(),
                        )
                    })?;

                Ok(StoredRelaySession {
                    session_id: row.get(0)?,
                    protocol: row.get(1)?,
                    parties,
                    started_at: row.get::<_, i64>(3)? as u64,
                    last_activity: row.get::<_, i64>(4)? as u64,
                    message_queues_json: row.get(5)?,
                    parties_ready,
                    parties_completed,
                    active: row.get::<_, i32>(8)? != 0,
                    error: row.get(9)?,
                })
            })
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| MpcWalletError::Storage(format!("Query error: {}", e)))?;

        Ok(sessions)
    }

    /// Delete a session by ID.
    pub fn delete_session(&self, session_id: &str) -> Result<bool, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let rows = conn
            .execute(
                "DELETE FROM relay_sessions WHERE session_id = ?1",
                params![session_id],
            )
            .map_err(|e| MpcWalletError::Storage(format!("Delete error: {}", e)))?;

        Ok(rows > 0)
    }

    /// Delete all expired sessions (based on TTL).
    pub fn cleanup_expired(&self, ttl_secs: u64) -> Result<usize, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cutoff = now.saturating_sub(ttl_secs) as i64;

        let rows = conn
            .execute(
                "DELETE FROM relay_sessions WHERE last_activity <= ?1",
                params![cutoff],
            )
            .map_err(|e| MpcWalletError::Storage(format!("Cleanup error: {}", e)))?;

        if rows > 0 {
            tracing::info!("Cleaned up {} expired relay sessions from storage", rows);
        }

        Ok(rows)
    }

    /// Get session count.
    pub fn session_count(&self) -> Result<usize, MpcWalletError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| MpcWalletError::Storage(format!("Lock error: {}", e)))?;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM relay_sessions", [], |row| row.get(0))
            .map_err(|e| MpcWalletError::Storage(format!("Count error: {}", e)))?;

        Ok(count as usize)
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn parse_wallet_type(s: &str) -> Result<WalletType, String> {
    match s.to_lowercase().as_str() {
        "bitcoin" => Ok(WalletType::Bitcoin),
        "taproot" => Ok(WalletType::Taproot),
        "ethereum" => Ok(WalletType::Ethereum),
        other => Err(format!("unknown wallet type: '{}'", other)),
    }
}

// Extend the optional trait for rusqlite
trait OptionalExt<T> {
    fn optional(self) -> SqlResult<Option<T>>;
}

impl<T> OptionalExt<T> for SqlResult<T> {
    fn optional(self) -> SqlResult<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_session_store_crud() {
        let store = RelaySessionStore::open_in_memory().unwrap();

        let session = StoredRelaySession {
            session_id: "test-session-1".to_string(),
            protocol: "signing".to_string(),
            parties: vec![0, 1, 2],
            started_at: 1000,
            last_activity: 1000,
            message_queues_json: "{}".to_string(),
            parties_ready: vec![0],
            parties_completed: vec![],
            active: true,
            error: None,
        };

        // Save
        store.save_session(&session).unwrap();

        // Get
        let loaded = store.get_session("test-session-1").unwrap().unwrap();
        assert_eq!(loaded.session_id, session.session_id);
        assert_eq!(loaded.protocol, session.protocol);
        assert_eq!(loaded.parties, session.parties);
        assert!(loaded.active);

        // Update
        let mut updated = session.clone();
        updated.parties_ready = vec![0, 1, 2];
        updated.active = false;
        store.save_session(&updated).unwrap();

        let loaded = store.get_session("test-session-1").unwrap().unwrap();
        assert_eq!(loaded.parties_ready, vec![0, 1, 2]);
        assert!(!loaded.active);

        // Count
        assert_eq!(store.session_count().unwrap(), 1);

        // Delete
        assert!(store.delete_session("test-session-1").unwrap());
        assert_eq!(store.session_count().unwrap(), 0);
    }

    #[test]
    fn test_relay_session_store_list_active() {
        let store = RelaySessionStore::open_in_memory().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create an active session (recent activity)
        let active_session = StoredRelaySession {
            session_id: "active-session".to_string(),
            protocol: "signing".to_string(),
            parties: vec![0, 1, 2],
            started_at: now,
            last_activity: now,
            message_queues_json: "{}".to_string(),
            parties_ready: vec![],
            parties_completed: vec![],
            active: true,
            error: None,
        };
        store.save_session(&active_session).unwrap();

        // Create an expired session (old activity)
        let expired_session = StoredRelaySession {
            session_id: "expired-session".to_string(),
            protocol: "signing".to_string(),
            parties: vec![0, 1, 2],
            started_at: now - 7200, // 2 hours ago
            last_activity: now - 7200,
            message_queues_json: "{}".to_string(),
            parties_ready: vec![],
            parties_completed: vec![],
            active: true,
            error: None,
        };
        store.save_session(&expired_session).unwrap();

        // List active sessions (TTL = 1 hour = 3600 seconds)
        let active = store.list_active_sessions(3600).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].session_id, "active-session");

        // Cleanup expired
        let cleaned = store.cleanup_expired(3600).unwrap();
        assert_eq!(cleaned, 1);
        assert_eq!(store.session_count().unwrap(), 1);
    }

    #[test]
    fn test_wallet_store_crud() {
        let store = WalletStore::open_in_memory().unwrap();

        let wallet = StoredWallet {
            id: Uuid::new_v4(),
            name: "Test Wallet".to_string(),
            wallet_type: WalletType::Bitcoin,
            public_key: "02abc123".to_string(),
            address: "tb1qtest".to_string(),
            created_at: Utc::now(),
        };

        // Save
        store.save_wallet(&wallet).unwrap();

        // Get
        let loaded = store.get_wallet(wallet.id).unwrap().unwrap();
        assert_eq!(loaded.name, wallet.name);
        assert_eq!(loaded.address, wallet.address);

        // List
        let wallets = store.list_wallets().unwrap();
        assert_eq!(wallets.len(), 1);

        // Count
        assert_eq!(store.wallet_count().unwrap(), 1);

        // Delete
        assert!(store.delete_wallet(wallet.id).unwrap());
        assert_eq!(store.wallet_count().unwrap(), 0);
    }

    #[test]
    fn test_key_share_store_crud() {
        let store = KeyShareStore::open_in_memory(0).unwrap();

        let share = StoredKeyShare {
            wallet_id: Uuid::new_v4(),
            party_index: 0,
            secret_share: "deadbeef".to_string(),
            public_key: "02abc123".to_string(),
            public_key_shares: "[\"02abc\", \"03def\"]".to_string(),
            created_at: Utc::now(),
        };

        // Save
        store.save_key_share(&share).unwrap();

        // Get
        let loaded = store.get_key_share(share.wallet_id).unwrap().unwrap();
        assert_eq!(loaded.secret_share, share.secret_share);

        // Has
        assert!(store.has_key_share(share.wallet_id).unwrap());

        // Delete
        assert!(store.delete_key_share(share.wallet_id).unwrap());
        assert!(!store.has_key_share(share.wallet_id).unwrap());
    }
}
