-- MPC Wallet PostgreSQL Schema
-- Version: 1.0.0
-- Date: 2026-01-20

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Transaction states are now stored as TEXT with CHECK constraint
-- No enum type needed, more flexible for future state additions

-- Byzantine violation types enum
CREATE TYPE violation_type AS ENUM (
    'double_vote',
    'invalid_signature',
    'timeout',
    'malformed_message'
);

-- Transactions table
CREATE TABLE IF NOT EXISTS transactions (
    id BIGSERIAL PRIMARY KEY,
    txid TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL DEFAULT 'pending',
    unsigned_tx BYTEA NOT NULL,
    signed_tx BYTEA,
    recipient TEXT NOT NULL,
    amount_sats BIGINT NOT NULL CHECK (amount_sats > 0),
    fee_sats BIGINT NOT NULL CHECK (fee_sats > 0),
    metadata TEXT,
    op_return_data BYTEA CHECK (octet_length(op_return_data) <= 80),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    bitcoin_txid TEXT,
    confirmations INTEGER DEFAULT 0,
    CONSTRAINT transactions_state_check CHECK (
        state IN (
            'pending', 'voting', 'collecting', 'threshold_reached', 'approved',
            'rejected', 'signing', 'signed', 'submitted', 'broadcasting',
            'confirmed', 'failed', 'aborted_byzantine'
        )
    ),
    CONSTRAINT valid_state_transition CHECK (
        (state = 'pending' AND signed_tx IS NULL) OR
        (state = 'signing') OR
        (state IN ('signed', 'broadcasting', 'confirmed') AND signed_tx IS NOT NULL) OR
        (state IN ('voting', 'approved', 'rejected', 'failed'))
    )
);

CREATE INDEX idx_transactions_state ON transactions(state);
CREATE INDEX idx_transactions_created_at ON transactions(created_at DESC);
CREATE INDEX idx_transactions_bitcoin_txid ON transactions(bitcoin_txid) WHERE bitcoin_txid IS NOT NULL;

-- Voting rounds table
CREATE TABLE IF NOT EXISTS voting_rounds (
    id BIGSERIAL PRIMARY KEY,
    tx_id TEXT NOT NULL REFERENCES transactions(txid) ON DELETE CASCADE,
    round_number INTEGER NOT NULL,
    total_nodes INTEGER NOT NULL CHECK (total_nodes > 0),
    threshold INTEGER NOT NULL CHECK (threshold > 0 AND threshold <= total_nodes),
    votes_received INTEGER NOT NULL DEFAULT 0 CHECK (votes_received >= 0),
    approved BOOLEAN NOT NULL DEFAULT FALSE,
    completed BOOLEAN NOT NULL DEFAULT FALSE,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    timeout_at TIMESTAMPTZ NOT NULL,
    UNIQUE(tx_id, round_number),
    CONSTRAINT valid_threshold CHECK (threshold > total_nodes / 2)
);

CREATE INDEX idx_voting_rounds_tx_id ON voting_rounds(tx_id);
CREATE INDEX idx_voting_rounds_completed ON voting_rounds(completed, completed_at);

-- Votes table
CREATE TABLE IF NOT EXISTS votes (
    id BIGSERIAL PRIMARY KEY,
    round_id BIGINT NOT NULL REFERENCES voting_rounds(id) ON DELETE CASCADE,
    node_id BIGINT NOT NULL,
    tx_id TEXT NOT NULL,
    approve BOOLEAN NOT NULL,
    value BIGINT,
    signature BYTEA NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(round_id, node_id),
    CONSTRAINT valid_node_id CHECK (node_id >= 0)
);

CREATE INDEX idx_votes_round_id ON votes(round_id);
CREATE INDEX idx_votes_node_id ON votes(node_id);
CREATE INDEX idx_votes_tx_id ON votes(tx_id);

-- Byzantine violations table
CREATE TABLE IF NOT EXISTS byzantine_violations (
    id BIGSERIAL PRIMARY KEY,
    node_id BIGINT NOT NULL,
    violation_type violation_type NOT NULL,
    round_id BIGINT REFERENCES voting_rounds(id) ON DELETE SET NULL,
    tx_id TEXT,
    evidence JSONB NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    action_taken TEXT,
    CONSTRAINT valid_evidence CHECK (jsonb_typeof(evidence) = 'object')
);

CREATE INDEX idx_byzantine_violations_node_id ON byzantine_violations(node_id);
CREATE INDEX idx_byzantine_violations_type ON byzantine_violations(violation_type);
CREATE INDEX idx_byzantine_violations_detected_at ON byzantine_violations(detected_at DESC);

-- Presignature usage table
CREATE TABLE IF NOT EXISTS presignature_usage (
    id BIGSERIAL PRIMARY KEY,
    presig_id UUID NOT NULL UNIQUE,
    transaction_id BIGINT NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    generation_time_ms INTEGER NOT NULL CHECK (generation_time_ms > 0),
    protocol TEXT NOT NULL CHECK (protocol IN ('cggmp24', 'frost')),
    CONSTRAINT valid_generation_time CHECK (generation_time_ms < 60000)
);

CREATE INDEX idx_presignature_usage_transaction_id ON presignature_usage(transaction_id);
CREATE INDEX idx_presignature_usage_used_at ON presignature_usage(used_at DESC);
CREATE INDEX idx_presignature_usage_protocol ON presignature_usage(protocol);

-- DKG ceremonies table (for tracking distributed key generation)
CREATE TABLE IF NOT EXISTS dkg_ceremonies (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    protocol TEXT NOT NULL CHECK (protocol IN ('cggmp24', 'frost')),
    threshold INTEGER NOT NULL CHECK (threshold > 0),
    total_nodes INTEGER NOT NULL CHECK (total_nodes > 0),
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    public_key BYTEA,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error TEXT,
    CONSTRAINT valid_dkg_threshold CHECK (threshold > total_nodes / 2 AND threshold <= total_nodes),
    CONSTRAINT valid_completion CHECK (
        (status = 'completed' AND public_key IS NOT NULL AND completed_at IS NOT NULL) OR
        (status = 'failed' AND completed_at IS NOT NULL) OR
        (status = 'running' AND completed_at IS NULL)
    )
);

CREATE INDEX idx_dkg_ceremonies_session_id ON dkg_ceremonies(session_id);
CREATE INDEX idx_dkg_ceremonies_protocol ON dkg_ceremonies(protocol);
CREATE INDEX idx_dkg_ceremonies_status ON dkg_ceremonies(status);
CREATE INDEX idx_dkg_ceremonies_started_at ON dkg_ceremonies(started_at DESC);

-- Key shares table (encrypted key shares per node)
CREATE TABLE IF NOT EXISTS key_shares (
    id BIGSERIAL PRIMARY KEY,
    ceremony_id BIGINT NOT NULL REFERENCES dkg_ceremonies(id) ON DELETE CASCADE,
    node_id BIGINT NOT NULL CHECK (node_id > 0),
    encrypted_share BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(ceremony_id, node_id)
);

CREATE INDEX idx_key_shares_ceremony_id ON key_shares(ceremony_id);
CREATE INDEX idx_key_shares_node_id ON key_shares(node_id);
CREATE INDEX idx_key_shares_created_at ON key_shares(created_at DESC);

-- Aux info table (auxiliary information for CGGMP24 signing)
CREATE TABLE IF NOT EXISTS aux_info (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    node_id BIGINT NOT NULL CHECK (node_id > 0),
    aux_info_data BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(session_id, node_id)
);

CREATE INDEX idx_aux_info_session_id ON aux_info(session_id);
CREATE INDEX idx_aux_info_node_id ON aux_info(node_id);
CREATE INDEX idx_aux_info_created_at ON aux_info(created_at DESC);

-- Aux info sessions table (tracking aux_info generation ceremonies)
CREATE TABLE IF NOT EXISTS aux_info_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    party_index INTEGER NOT NULL,
    num_parties INTEGER NOT NULL CHECK (num_parties > 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'generating_primes', 'running', 'completed', 'failed')),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error TEXT,
    CONSTRAINT valid_party_index CHECK (party_index >= 0 AND party_index < num_parties)
);

CREATE INDEX idx_aux_info_sessions_session_id ON aux_info_sessions(session_id);
CREATE INDEX idx_aux_info_sessions_status ON aux_info_sessions(status);
CREATE INDEX idx_aux_info_sessions_started_at ON aux_info_sessions(started_at DESC);

-- Node status table (for tracking node health)
CREATE TABLE IF NOT EXISTS node_status (
    id BIGSERIAL PRIMARY KEY,
    node_id BIGINT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('online', 'offline', 'degraded', 'banned')),
    last_heartbeat TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    total_votes BIGINT NOT NULL DEFAULT 0,
    total_violations BIGINT NOT NULL DEFAULT 0,
    banned_until TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT valid_node_id CHECK (node_id >= 0)
);

CREATE INDEX idx_node_status_status ON node_status(status);
CREATE INDEX idx_node_status_last_heartbeat ON node_status(last_heartbeat DESC);

-- Audit log table (immutable, append-only)
CREATE TABLE IF NOT EXISTS audit_log (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    node_id BIGINT,
    tx_id TEXT,
    details JSONB NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT valid_details CHECK (jsonb_typeof(details) = 'object')
);

CREATE INDEX idx_audit_log_event_type ON audit_log(event_type);
CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp DESC);
CREATE INDEX idx_audit_log_tx_id ON audit_log(tx_id) WHERE tx_id IS NOT NULL;
CREATE INDEX idx_audit_log_node_id ON audit_log(node_id) WHERE node_id IS NOT NULL;

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for transactions table
CREATE TRIGGER update_transactions_updated_at
    BEFORE UPDATE ON transactions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Trigger for node_status table
CREATE TRIGGER update_node_status_updated_at
    BEFORE UPDATE ON node_status
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Function to log state transitions
CREATE OR REPLACE FUNCTION log_transaction_state_change()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.state IS DISTINCT FROM NEW.state THEN
        INSERT INTO audit_log (event_type, tx_id, details)
        VALUES (
            'transaction_state_change',
            NEW.txid,
            jsonb_build_object(
                'old_state', OLD.state,
                'new_state', NEW.state,
                'transaction_id', NEW.id
            )
        );
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for transaction state changes
CREATE TRIGGER log_transaction_state_change_trigger
    AFTER UPDATE ON transactions
    FOR EACH ROW
    WHEN (OLD.state IS DISTINCT FROM NEW.state)
    EXECUTE FUNCTION log_transaction_state_change();

-- Function to increment node violation count
CREATE OR REPLACE FUNCTION increment_node_violations()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE node_status
    SET total_violations = total_violations + 1
    WHERE node_id = NEW.node_id;

    -- Auto-ban after 5 violations
    UPDATE node_status
    SET status = 'banned', banned_until = NOW() + INTERVAL '24 hours'
    WHERE node_id = NEW.node_id AND total_violations >= 5 AND status != 'banned';

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for Byzantine violations
CREATE TRIGGER increment_node_violations_trigger
    AFTER INSERT ON byzantine_violations
    FOR EACH ROW
    EXECUTE FUNCTION increment_node_violations();

-- View for transaction summary
CREATE OR REPLACE VIEW transaction_summary AS
SELECT
    t.id,
    t.txid,
    t.state,
    t.recipient,
    t.amount_sats,
    t.fee_sats,
    t.created_at,
    t.completed_at,
    t.bitcoin_txid,
    t.confirmations,
    vr.round_number,
    vr.votes_received,
    vr.threshold,
    vr.approved,
    COUNT(v.id) as total_votes
FROM transactions t
LEFT JOIN voting_rounds vr ON t.txid = vr.tx_id
LEFT JOIN votes v ON vr.id = v.round_id
GROUP BY t.id, t.txid, t.state, t.recipient, t.amount_sats, t.fee_sats,
         t.created_at, t.completed_at, t.bitcoin_txid, t.confirmations,
         vr.round_number, vr.votes_received, vr.threshold, vr.approved;

-- View for node health
CREATE OR REPLACE VIEW node_health AS
SELECT
    ns.node_id,
    ns.status,
    ns.last_heartbeat,
    ns.total_votes,
    ns.total_violations,
    ns.banned_until,
    COUNT(DISTINCT v.id) as votes_cast,
    COUNT(DISTINCT bv.id) as violations_recorded,
    EXTRACT(EPOCH FROM (NOW() - ns.last_heartbeat)) as seconds_since_heartbeat
FROM node_status ns
LEFT JOIN votes v ON ns.node_id = v.node_id
LEFT JOIN byzantine_violations bv ON ns.node_id = bv.node_id
GROUP BY ns.node_id, ns.status, ns.last_heartbeat, ns.total_votes,
         ns.total_violations, ns.banned_until;

COMMENT ON TABLE transactions IS 'Bitcoin transactions managed by MPC wallet';
COMMENT ON TABLE voting_rounds IS 'Consensus voting rounds for transaction approval';
COMMENT ON TABLE votes IS 'Individual votes from nodes in voting rounds';
COMMENT ON TABLE byzantine_violations IS 'Detected Byzantine fault tolerance violations';
COMMENT ON TABLE presignature_usage IS 'Tracking of presignature pool usage for fast signing';
COMMENT ON TABLE node_status IS 'Real-time status of all nodes in the network';
COMMENT ON TABLE audit_log IS 'Immutable audit trail for compliance';
COMMENT ON TABLE dkg_ceremonies IS 'Distributed key generation ceremonies for CGGMP24 and FROST protocols';
COMMENT ON TABLE key_shares IS 'Encrypted threshold key shares stored per node after DKG';
