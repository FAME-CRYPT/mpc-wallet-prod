# Migration to Event-Driven Architecture

**Status:** Planned
**Priority:** High (Performance & Scalability)
**Estimated Effort:** Medium
**Impact:** Eliminates database polling overhead, reduces latency from 5s to <10ms

---

## Current Problem: Polling-Based Architecture

### Existing Implementation

**OrchestrationService** (`production/crates/orchestrator/src/service.rs:115`)
```rust
let mut interval = interval(self.config.poll_interval); // 5 seconds
loop {
    interval.tick().await;
    self.process_pending_transactions().await;  // ❌ DB query every 5s
    self.process_voting_transactions().await;   // ❌ DB query every 5s
}
```

**TimeoutMonitor** (`production/crates/orchestrator/src/timeout_monitor.rs:59`)
```rust
let mut interval = interval(Duration::from_secs(10)); // 10 seconds
loop {
    interval.tick().await;
    self.check_voting_timeouts().await;      // ❌ DB query every 10s
    self.check_signing_timeouts().await;     // ❌ DB query every 10s
    self.check_broadcasting_timeouts().await;// ❌ DB query every 10s
}
```

### Performance Issues

1. **Database Load**
   - 5 nodes × 2 services × (every 5-10s) = 1-2 queries/sec per node
   - With 1000 pending transactions: Each query scans 1000 rows
   - Even with indexes: Unnecessary CPU, I/O, network overhead

2. **Latency**
   - Transaction created → Up to 5 seconds before processing starts
   - Timeout occurred → Up to 10 seconds before detection
   - Not real-time, only near-real-time

3. **Scalability Limits**
   - 10,000 transactions/minute → Database overload
   - Reducing poll interval (e.g., 1s) → 5x database load
   - Adding more nodes → Each node duplicates same queries

---

## Solution: PostgreSQL LISTEN/NOTIFY

PostgreSQL's native pub/sub mechanism provides:
- ✅ **Instant notifications** (0ms latency)
- ✅ **Zero polling overhead** (database stays idle)
- ✅ **Simple implementation** (native PostgreSQL feature)
- ✅ **Reliable** (works within transactions)
- ✅ **No extra infrastructure** (no Redis/Kafka needed)

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                     PostgreSQL Database                      │
│                                                              │
│  ┌────────────────┐         ┌─────────────────────────┐    │
│  │  transactions  │────────▶│  Trigger Function       │    │
│  │  table         │         │  notify_tx_change()     │    │
│  └────────────────┘         └──────────┬──────────────┘    │
│                                         │                    │
│  ┌────────────────┐                    │                    │
│  │ voting_rounds  │────────────────────┤                    │
│  │  table         │                    │                    │
│  └────────────────┘                    ▼                    │
│                              ┌──────────────────┐           │
│                              │  NOTIFY channel  │           │
│                              │  'tx_events'     │           │
│                              └─────────┬────────┘           │
└────────────────────────────────────────┼────────────────────┘
                                         │
                                         │ pg_notify()
                                         │
                    ┌────────────────────┴────────────────────┐
                    │                                         │
            ┌───────▼────────┐                       ┌───────▼────────┐
            │   Node 1       │                       │   Node 2       │
            │   LISTEN       │                       │   LISTEN       │
            │                │         ...           │                │
            │ OrchestrationSvc│                      │ OrchestrationSvc│
            └────────────────┘                       └────────────────┘
```

---

## Implementation Steps

### Step 1: PostgreSQL Database Changes

Create a migration file: `production/migrations/YYYYMMDD_add_event_notifications.sql`

```sql
-- ============================================================================
-- Event Notification System for Transaction State Changes
-- ============================================================================
-- This migration adds PostgreSQL NOTIFY triggers to eliminate polling overhead
-- and enable real-time event-driven transaction processing.
--
-- Benefits:
-- - Eliminates 5-second polling latency
-- - Reduces database load by 90%+
-- - Enables instant transaction processing
-- - No additional infrastructure required
-- ============================================================================

-- Create notification channel for transaction events
-- Each event payload is JSON with: {txid, state, action, timestamp}

CREATE OR REPLACE FUNCTION notify_transaction_change()
RETURNS TRIGGER AS $$
DECLARE
    notification_payload json;
BEGIN
    -- Build JSON payload with transaction details
    notification_payload = json_build_object(
        'txid', COALESCE(NEW.txid, OLD.txid),
        'state', COALESCE(NEW.state, OLD.state),
        'action', TG_OP,  -- INSERT, UPDATE, DELETE
        'timestamp', EXTRACT(EPOCH FROM NOW())::bigint,
        'old_state', CASE WHEN TG_OP = 'UPDATE' THEN OLD.state ELSE NULL END
    );

    -- Send notification to 'tx_events' channel
    PERFORM pg_notify('tx_events', notification_payload::text);

    -- Return appropriate row based on operation
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    ELSE
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Create trigger for transaction state changes
DROP TRIGGER IF EXISTS transaction_change_trigger ON transactions;
CREATE TRIGGER transaction_change_trigger
    AFTER INSERT OR UPDATE OR DELETE ON transactions
    FOR EACH ROW
    EXECUTE FUNCTION notify_transaction_change();

-- Create notification function for voting round changes
CREATE OR REPLACE FUNCTION notify_voting_change()
RETURNS TRIGGER AS $$
DECLARE
    notification_payload json;
BEGIN
    notification_payload = json_build_object(
        'txid', COALESCE(NEW.txid, OLD.txid),
        'round_id', COALESCE(NEW.id, OLD.id),
        'votes_received', COALESCE(NEW.votes_received, OLD.votes_received),
        'threshold_reached', COALESCE(NEW.threshold_reached, OLD.threshold_reached),
        'action', TG_OP,
        'timestamp', EXTRACT(EPOCH FROM NOW())::bigint
    );

    PERFORM pg_notify('voting_events', notification_payload::text);

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    ELSE
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Create trigger for voting round changes
DROP TRIGGER IF EXISTS voting_change_trigger ON voting_rounds;
CREATE TRIGGER voting_change_trigger
    AFTER INSERT OR UPDATE OR DELETE ON voting_rounds
    FOR EACH ROW
    EXECUTE FUNCTION notify_voting_change();

-- Add indexes for efficient event filtering (if not already present)
CREATE INDEX IF NOT EXISTS idx_transactions_state_created
    ON transactions(state, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_voting_rounds_txid_threshold
    ON voting_rounds(txid, threshold_reached);

-- ============================================================================
-- Testing the notification system
-- ============================================================================
-- To verify notifications are working:
--
-- Terminal 1 (Listen for events):
--   psql -U mpc -d mpc_wallet -c "LISTEN tx_events;"
--
-- Terminal 2 (Trigger an event):
--   psql -U mpc -d mpc_wallet -c "
--     INSERT INTO transactions (txid, state, unsigned_tx, recipient, amount_sats, fee_sats)
--     VALUES ('test-tx-123', 'pending', E'\\x00', 'tb1q...', 10000, 1000);
--   "
--
-- Expected output in Terminal 1:
--   Asynchronous notification "tx_events" received from server process with PID 12345.
--   Payload: {"txid":"test-tx-123","state":"pending","action":"INSERT",...}
-- ============================================================================
```

**Rollback migration** (`production/migrations/YYYYMMDD_add_event_notifications_down.sql`):
```sql
-- Remove triggers
DROP TRIGGER IF EXISTS transaction_change_trigger ON transactions;
DROP TRIGGER IF EXISTS voting_change_trigger ON voting_rounds;

-- Remove functions
DROP FUNCTION IF EXISTS notify_transaction_change();
DROP FUNCTION IF EXISTS notify_voting_change();
```

---

### Step 2: Create Event Types in Rust

Create `production/crates/orchestrator/src/events.rs`:

```rust
//! Event-driven notification system for transaction state changes.
//!
//! Replaces polling-based architecture with PostgreSQL LISTEN/NOTIFY.

use serde::{Deserialize, Serialize};
use threshold_types::TxId;

/// Transaction state change event from PostgreSQL NOTIFY
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionEvent {
    /// Transaction ID
    pub txid: String,

    /// New state (pending, voting, signing, broadcasting, completed, failed)
    pub state: String,

    /// Database operation (INSERT, UPDATE, DELETE)
    pub action: String,

    /// Unix timestamp when event occurred
    pub timestamp: i64,

    /// Previous state (only for UPDATE operations)
    #[serde(default)]
    pub old_state: Option<String>,
}

impl TransactionEvent {
    /// Convert txid string to TxId type
    pub fn tx_id(&self) -> TxId {
        TxId(self.txid.clone())
    }

    /// Check if this is a state transition
    pub fn is_state_transition(&self) -> bool {
        self.action == "UPDATE" && self.old_state.is_some()
    }

    /// Check if transaction entered a specific state
    pub fn entered_state(&self, state: &str) -> bool {
        self.state == state &&
        (self.action == "INSERT" ||
         self.old_state.as_deref() != Some(state))
    }
}

/// Voting round change event from PostgreSQL NOTIFY
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingEvent {
    /// Transaction ID
    pub txid: String,

    /// Voting round ID
    pub round_id: i64,

    /// Number of votes received
    pub votes_received: i32,

    /// Whether threshold was reached
    pub threshold_reached: bool,

    /// Database operation
    pub action: String,

    /// Unix timestamp
    pub timestamp: i64,
}

impl VotingEvent {
    pub fn tx_id(&self) -> TxId {
        TxId(self.txid.clone())
    }
}

/// Combined event type for all notification channels
#[derive(Debug, Clone)]
pub enum DatabaseEvent {
    Transaction(TransactionEvent),
    Voting(VotingEvent),
}
```

---

### Step 3: Implement Event Listener

Add to `production/crates/orchestrator/src/event_listener.rs`:

```rust
//! PostgreSQL LISTEN/NOTIFY event listener
//!
//! Listens for database notifications and forwards them to the orchestration service.

use crate::error::{OrchestrationError, Result};
use crate::events::{DatabaseEvent, TransactionEvent, VotingEvent};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_postgres::{AsyncMessage, Connection, NoTls, Socket};
use tokio_postgres::tls::NoTlsStream;
use tracing::{debug, error, info, warn};

/// Event listener for PostgreSQL NOTIFY
pub struct EventListener {
    /// Database connection string
    connection_string: String,

    /// Channel to send events to orchestrator
    event_tx: mpsc::UnboundedSender<DatabaseEvent>,

    /// Shutdown signal
    shutdown: Arc<tokio::sync::RwLock<bool>>,
}

impl EventListener {
    /// Create a new event listener
    pub fn new(
        connection_string: String,
        event_tx: mpsc::UnboundedSender<DatabaseEvent>,
    ) -> Self {
        Self {
            connection_string,
            event_tx,
            shutdown: Arc::new(tokio::sync::RwLock::new(false)),
        }
    }

    /// Start listening for events
    pub async fn start(self: Arc<Self>) -> Result<()> {
        info!("Starting PostgreSQL event listener");

        loop {
            // Check shutdown signal
            if *self.shutdown.read().await {
                info!("Event listener shutting down");
                break;
            }

            // Connect and listen
            match self.connect_and_listen().await {
                Ok(()) => {
                    info!("Event listener stopped normally");
                    break;
                }
                Err(e) => {
                    error!("Event listener error: {}, reconnecting in 5s...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }

        Ok(())
    }

    /// Connect to PostgreSQL and listen for notifications
    async fn connect_and_listen(&self) -> Result<()> {
        // Connect to PostgreSQL
        let (client, mut connection) = tokio_postgres::connect(&self.connection_string, NoTls)
            .await
            .map_err(|e| OrchestrationError::Internal(format!("Failed to connect to PostgreSQL: {}", e)))?;

        info!("Connected to PostgreSQL for event listening");

        // Start listening on both channels
        client.execute("LISTEN tx_events", &[])
            .await
            .map_err(|e| OrchestrationError::Internal(format!("Failed to LISTEN tx_events: {}", e)))?;

        client.execute("LISTEN voting_events", &[])
            .await
            .map_err(|e| OrchestrationError::Internal(format!("Failed to LISTEN voting_events: {}", e)))?;

        info!("Listening on channels: tx_events, voting_events");

        // Process notifications
        while let Some(message) = connection.next().await {
            match message {
                Ok(AsyncMessage::Notification(notif)) => {
                    debug!("Received notification on channel: {}", notif.channel());

                    // Parse and forward event
                    if let Err(e) = self.handle_notification(notif.channel(), notif.payload()).await {
                        warn!("Failed to handle notification: {}", e);
                    }
                }
                Ok(AsyncMessage::Notice(notice)) => {
                    debug!("PostgreSQL notice: {}", notice);
                }
                Err(e) => {
                    error!("Connection error: {}", e);
                    return Err(OrchestrationError::Internal(format!("Connection error: {}", e)));
                }
                _ => {}
            }

            // Check shutdown signal
            if *self.shutdown.read().await {
                break;
            }
        }

        Ok(())
    }

    /// Handle a single notification
    async fn handle_notification(&self, channel: &str, payload: &str) -> Result<()> {
        let event = match channel {
            "tx_events" => {
                let tx_event: TransactionEvent = serde_json::from_str(payload)
                    .map_err(|e| OrchestrationError::Internal(format!("Failed to parse tx_event: {}", e)))?;

                debug!(
                    "Transaction event: txid={} state={} action={}",
                    tx_event.txid, tx_event.state, tx_event.action
                );

                DatabaseEvent::Transaction(tx_event)
            }
            "voting_events" => {
                let voting_event: VotingEvent = serde_json::from_str(payload)
                    .map_err(|e| OrchestrationError::Internal(format!("Failed to parse voting_event: {}", e)))?;

                debug!(
                    "Voting event: txid={} votes={} threshold_reached={}",
                    voting_event.txid, voting_event.votes_received, voting_event.threshold_reached
                );

                DatabaseEvent::Voting(voting_event)
            }
            _ => {
                warn!("Unknown notification channel: {}", channel);
                return Ok(());
            }
        };

        // Forward event to orchestrator
        self.event_tx.send(event)
            .map_err(|e| OrchestrationError::Internal(format!("Failed to send event: {}", e)))?;

        Ok(())
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) {
        info!("Shutting down event listener");
        *self.shutdown.write().await = true;
    }
}
```

---

### Step 4: Modify OrchestrationService

Update `production/crates/orchestrator/src/service.rs`:

**Remove polling loop**, replace with event-driven processing:

```rust
use crate::events::{DatabaseEvent, TransactionEvent, VotingEvent};
use crate::event_listener::EventListener;

impl OrchestrationService {
    /// Start the orchestration service (event-driven mode)
    pub fn start(self: Arc<Self>, event_rx: mpsc::UnboundedReceiver<DatabaseEvent>) -> JoinHandle<Result<()>> {
        info!("Starting event-driven transaction orchestration service");

        tokio::spawn(async move {
            match self.run_event_driven(event_rx).await {
                Ok(()) => {
                    info!("Orchestration service stopped normally");
                    Ok(())
                }
                Err(e) => {
                    error!("Orchestration service error: {}", e);
                    Err(e)
                }
            }
        })
    }

    /// Event-driven orchestration loop
    async fn run_event_driven(&self, mut event_rx: mpsc::UnboundedReceiver<DatabaseEvent>) -> Result<()> {
        info!("Event-driven orchestration started");

        while let Some(event) = event_rx.recv().await {
            // Check shutdown signal
            if *self.shutdown.read().await {
                info!("Orchestration service shutting down");
                break;
            }

            // Process event
            if let Err(e) = self.handle_database_event(event).await {
                error!("Error handling database event: {}", e);
            }
        }

        Ok(())
    }

    /// Handle a database event
    async fn handle_database_event(&self, event: DatabaseEvent) -> Result<()> {
        match event {
            DatabaseEvent::Transaction(tx_event) => {
                self.handle_transaction_event(tx_event).await
            }
            DatabaseEvent::Voting(voting_event) => {
                self.handle_voting_event(voting_event).await
            }
        }
    }

    /// Handle transaction state change event
    async fn handle_transaction_event(&self, event: TransactionEvent) -> Result<()> {
        let tx_id = event.tx_id();

        match event.state.as_str() {
            "pending" => {
                if event.entered_state("pending") {
                    info!("Transaction {} entered pending state, processing...", tx_id);
                    self.process_single_pending_transaction(tx_id).await?;
                }
            }
            "voting" => {
                if event.entered_state("voting") {
                    info!("Transaction {} entered voting state", tx_id);
                    // Voting is handled by voting_events channel
                }
            }
            "signing" => {
                if event.entered_state("signing") {
                    info!("Transaction {} entered signing state, coordinating signature...", tx_id);
                    self.initiate_signing(tx_id).await?;
                }
            }
            "broadcasting" => {
                if event.entered_state("broadcasting") {
                    info!("Transaction {} entered broadcasting state", tx_id);
                    self.broadcast_transaction(tx_id).await?;
                }
            }
            "completed" | "failed" => {
                debug!("Transaction {} finished with state: {}", tx_id, event.state);
            }
            _ => {
                warn!("Unknown transaction state: {}", event.state);
            }
        }

        Ok(())
    }

    /// Handle voting round event
    async fn handle_voting_event(&self, event: VotingEvent) -> Result<()> {
        if event.threshold_reached {
            let tx_id = event.tx_id();
            info!("Voting threshold reached for transaction {}, transitioning to signing", tx_id);

            // Transition from voting to signing
            self.postgres.update_transaction_state(&tx_id, TransactionState::Signing).await
                .map_err(|e| OrchestrationError::Storage(e.into()))?;
        }

        Ok(())
    }

    /// Process a single pending transaction (called by event handler)
    async fn process_single_pending_transaction(&self, tx_id: TxId) -> Result<()> {
        // Reuse existing logic from process_pending_transactions()
        // but for a single transaction instead of batch

        // ... existing voting initiation logic ...

        Ok(())
    }
}
```

---

### Step 5: Update TimeoutMonitor

The timeout monitor can continue polling (it's checking time-based conditions, not data changes), but reduce frequency:

```rust
// Change from 10 seconds to 30 seconds (less critical now that events are instant)
let mut interval = interval(Duration::from_secs(30));
```

Or convert it to event-based using database jobs/cron, but that's a later optimization.

---

### Step 6: Wire Everything Together

Update `production/crates/server/src/main.rs`:

```rust
use threshold_orchestrator::event_listener::EventListener;
use threshold_orchestrator::events::DatabaseEvent;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    // ... existing initialization ...

    // Create event channel
    let (event_tx, event_rx) = mpsc::unbounded_channel::<DatabaseEvent>();

    // Start event listener
    let event_listener = Arc::new(EventListener::new(
        postgres_url.clone(),
        event_tx,
    ));

    let listener_handle = {
        let listener = Arc::clone(&event_listener);
        tokio::spawn(async move {
            if let Err(e) = listener.start().await {
                error!("Event listener failed: {}", e);
            }
        })
    };

    // Start orchestration service with event channel
    let orchestration_handle = orchestration_service.start(event_rx);

    // ... rest of startup ...

    // Graceful shutdown
    tokio::select! {
        _ = shutdown_signal() => {
            info!("Shutdown signal received");
            event_listener.shutdown().await;
            orchestration_service.shutdown().await;
        }
    }

    // Wait for services to stop
    let _ = tokio::join!(listener_handle, orchestration_handle);

    Ok(())
}
```

---

## Testing Strategy

### 1. Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_transaction_event() {
        let payload = r#"{
            "txid": "test-tx-123",
            "state": "pending",
            "action": "INSERT",
            "timestamp": 1234567890
        }"#;

        let event: TransactionEvent = serde_json::from_str(payload).unwrap();
        assert_eq!(event.txid, "test-tx-123");
        assert_eq!(event.state, "pending");
        assert!(event.entered_state("pending"));
    }

    #[tokio::test]
    async fn test_event_listener_reconnect() {
        // Test that listener reconnects after connection failure
    }
}
```

### 2. Integration Tests

```bash
# Terminal 1: Start service
docker-compose up

# Terminal 2: Monitor events
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "LISTEN tx_events;"

# Terminal 3: Create transaction
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{"recipient":"tb1q...", "amount_sats":10000}'

# Expected: Terminal 2 shows notification immediately
# Expected: Service logs show "Transaction entered pending state, processing..."
```

### 3. Performance Tests

Before/After comparison:
```bash
# Before (polling):
# - Check database CPU usage
# - Measure transaction processing latency
# - Count queries/second

# After (events):
# - Database CPU should drop ~80%
# - Latency should drop from 5s avg to <100ms
# - Queries/second should drop ~90%
```

---

## Rollback Plan

If issues occur:

1. **Immediate rollback:**
   ```sql
   -- Disable triggers
   ALTER TABLE transactions DISABLE TRIGGER transaction_change_trigger;
   ALTER TABLE voting_rounds DISABLE TRIGGER voting_change_trigger;
   ```

2. **Revert code:**
   ```bash
   git revert <commit-hash>
   docker-compose build --no-cache
   docker-compose up -d
   ```

3. **Full rollback:**
   ```bash
   # Run down migration
   psql -U mpc -d mpc_wallet -f migrations/YYYYMMDD_add_event_notifications_down.sql
   ```

---

## Monitoring & Observability

### Metrics to Track

```rust
// Add to prometheus metrics
pub static EVENT_NOTIFICATIONS_RECEIVED: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "event_notifications_received_total",
        "Total number of database event notifications received",
        &["channel", "action"]
    ).unwrap()
});

pub static EVENT_PROCESSING_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "event_processing_duration_seconds",
        "Time to process database event",
        &["event_type"]
    ).unwrap()
});

pub static EVENT_LISTENER_RECONNECTS: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "event_listener_reconnects_total",
        "Number of times event listener reconnected"
    ).unwrap()
});
```

### Logging

```rust
// Key log points
info!("📨 Transaction event: txid={} state={} latency={}ms", ...);
warn!("⚠️  Event listener reconnecting after error: {}", ...);
error!("❌ Failed to process event: txid={} error={}", ...);
```

### Alerts

```yaml
# Prometheus alerts
- alert: EventListenerDown
  expr: time() - event_listener_last_heartbeat > 60
  for: 1m
  annotations:
    summary: "Event listener not receiving notifications"

- alert: EventProcessingLag
  expr: event_processing_duration_seconds{quantile="0.99"} > 1
  for: 5m
  annotations:
    summary: "Event processing taking >1s at p99"
```

---

## Future Enhancements

### Phase 2: Event Sourcing (Optional)

Store all events in an append-only log:
```sql
CREATE TABLE event_log (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ DEFAULT NOW(),
    processed BOOLEAN DEFAULT FALSE
);

-- Trigger writes to both NOTIFY and event_log
-- Enables replay, audit trail, debugging
```

### Phase 3: Cross-Node Coordination (Optional)

If you need to ensure only one node processes each event:
```rust
// Use etcd to acquire event lock
let lock_key = format!("/event-locks/{}", txid);
if etcd.acquire_lock(&lock_key, 10).await? {
    // Process event
    process_transaction(txid).await?;
    etcd.release_lock(&lock_key).await?;
} else {
    // Another node is handling it
    debug!("Event already being processed by another node");
}
```

---

## Success Criteria

✅ **Must Have:**
- Database polling loops removed from OrchestrationService
- Event listener successfully receives notifications
- Transactions process within 100ms of creation
- No dropped events during normal operation
- Graceful reconnection after database restart

✅ **Nice to Have:**
- Database CPU usage reduced by >70%
- Query rate reduced by >80%
- P99 processing latency <500ms
- Event replay capability for debugging

---

## Dependencies

### Rust Crates
```toml
[dependencies]
tokio-postgres = { version = "0.7", features = ["with-serde_json-1"] }
serde_json = "1.0"
```

### Database
- PostgreSQL 12+ (LISTEN/NOTIFY available since PostgreSQL 9.0)
- No new dependencies

---

## References

- [PostgreSQL NOTIFY Documentation](https://www.postgresql.org/docs/current/sql-notify.html)
- [tokio-postgres Notifications](https://docs.rs/tokio-postgres/latest/tokio_postgres/)
- [Event-Driven Architecture Patterns](https://martinfowler.com/articles/201701-event-driven.html)

---

**Last Updated:** 2026-01-27
**Status:** Ready for Implementation
**Assigned To:** TBD
