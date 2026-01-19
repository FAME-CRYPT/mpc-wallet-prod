#!/bin/bash
set -e

echo "Starting PostgreSQL initialization..."
echo "Database: $POSTGRES_DB"
echo "User: $POSTGRES_USER"

# POSTGRES_DB environment variable already creates the database
# We just need to create tables in the database

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    -- Node reputation table
    CREATE TABLE IF NOT EXISTS node_reputation (
        peer_id TEXT PRIMARY KEY,
        reputation_score DOUBLE PRECISION NOT NULL DEFAULT 1.0,
        total_votes BIGINT NOT NULL DEFAULT 0,
        violations BIGINT NOT NULL DEFAULT 0,
        last_updated TIMESTAMP WITH TIME ZONE DEFAULT NOW()
    );

    -- Vote history table
    CREATE TABLE IF NOT EXISTS vote_history (
        id BIGSERIAL PRIMARY KEY,
        tx_id TEXT NOT NULL,
        node_id TEXT NOT NULL,
        peer_id TEXT NOT NULL,
        value BIGINT NOT NULL,
        signature BYTEA NOT NULL,
        public_key BYTEA NOT NULL,
        timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        UNIQUE(tx_id, node_id)
    );

    -- Byzantine violations table
    CREATE TABLE IF NOT EXISTS byzantine_violations (
        id BIGSERIAL PRIMARY KEY,
        peer_id TEXT NOT NULL,
        violation_type TEXT NOT NULL,
        tx_id TEXT NOT NULL,
        details TEXT,
        detected_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        banned BOOLEAN DEFAULT TRUE
    );

    -- Blockchain submissions table
    CREATE TABLE IF NOT EXISTS blockchain_submissions (
        id BIGSERIAL PRIMARY KEY,
        tx_id TEXT NOT NULL UNIQUE,
        consensus_value BIGINT NOT NULL,
        threshold_reached BIGINT NOT NULL,
        total_votes BIGINT NOT NULL,
        participating_nodes TEXT[] NOT NULL,
        state TEXT NOT NULL DEFAULT 'PENDING',
        created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        confirmed_at TIMESTAMP WITH TIME ZONE
    );

    -- Archive table for old submissions
    CREATE TABLE IF NOT EXISTS blockchain_submissions_archive (
        LIKE blockchain_submissions INCLUDING ALL
    );

    -- Indexes for performance
    CREATE INDEX IF NOT EXISTS idx_vote_history_tx_id ON vote_history(tx_id);
    CREATE INDEX IF NOT EXISTS idx_vote_history_peer_id ON vote_history(peer_id);
    CREATE INDEX IF NOT EXISTS idx_byzantine_violations_peer_id ON byzantine_violations(peer_id);
    CREATE INDEX IF NOT EXISTS idx_blockchain_submissions_state ON blockchain_submissions(state);
    CREATE INDEX IF NOT EXISTS idx_blockchain_submissions_created_at ON blockchain_submissions(created_at);

    GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO $POSTGRES_USER;
    GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO $POSTGRES_USER;
EOSQL

echo "All tables created successfully in database: $POSTGRES_DB"
