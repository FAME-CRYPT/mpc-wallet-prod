#!/bin/sh
set -e

# PostgreSQL Initialization Script
# This script runs after the schema.sql file has been applied
# It performs additional setup tasks

echo "Running MPC Wallet database initialization..."

# Wait for PostgreSQL to be fully ready
until PGPASSWORD="${POSTGRES_PASSWORD}" psql -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" -c '\q' 2>/dev/null; do
  echo "PostgreSQL is unavailable - sleeping"
  sleep 1
done

echo "PostgreSQL is ready - executing initialization"

# Create additional indexes for performance (if not in schema.sql)
PGPASSWORD="${POSTGRES_PASSWORD}" psql -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" <<-EOSQL
    -- Additional performance optimizations

    -- Enable query planning statistics
    ANALYZE;

    -- Create function to cleanup old audit logs (optional)
    CREATE OR REPLACE FUNCTION cleanup_old_audit_logs(days INTEGER DEFAULT 90)
    RETURNS INTEGER AS \$\$
    DECLARE
        deleted_count INTEGER;
    BEGIN
        DELETE FROM audit_log
        WHERE timestamp < NOW() - (days || ' days')::INTERVAL;
        GET DIAGNOSTICS deleted_count = ROW_COUNT;
        RETURN deleted_count;
    END;
    \$\$ LANGUAGE plpgsql;

    -- Create function to get transaction statistics
    CREATE OR REPLACE FUNCTION get_transaction_stats()
    RETURNS TABLE (
        total_transactions BIGINT,
        pending_transactions BIGINT,
        confirmed_transactions BIGINT,
        failed_transactions BIGINT,
        avg_confirmation_time INTERVAL
    ) AS \$\$
    BEGIN
        RETURN QUERY
        SELECT
            COUNT(*)::BIGINT as total_transactions,
            COUNT(*) FILTER (WHERE state = 'pending')::BIGINT as pending_transactions,
            COUNT(*) FILTER (WHERE state = 'confirmed')::BIGINT as confirmed_transactions,
            COUNT(*) FILTER (WHERE state = 'failed')::BIGINT as failed_transactions,
            AVG(completed_at - created_at) FILTER (WHERE completed_at IS NOT NULL) as avg_confirmation_time
        FROM transactions;
    END;
    \$\$ LANGUAGE plpgsql;

    -- Grant execute permissions
    GRANT EXECUTE ON FUNCTION cleanup_old_audit_logs(INTEGER) TO ${POSTGRES_USER};
    GRANT EXECUTE ON FUNCTION get_transaction_stats() TO ${POSTGRES_USER};

    -- Create initial node status entries for 5 nodes
    INSERT INTO node_status (node_id, status, last_heartbeat, total_votes, total_violations)
    VALUES
        (1, 'offline', NOW(), 0, 0),
        (2, 'offline', NOW(), 0, 0),
        (3, 'offline', NOW(), 0, 0),
        (4, 'offline', NOW(), 0, 0),
        (5, 'offline', NOW(), 0, 0)
    ON CONFLICT (node_id) DO NOTHING;

    -- Log initialization event
    INSERT INTO audit_log (event_type, details)
    VALUES ('database_initialized', jsonb_build_object(
        'timestamp', NOW(),
        'version', '1.0.0',
        'nodes', 5
    ));

    -- Display initialization summary
    SELECT
        'Initialization Complete' as status,
        COUNT(*) as total_tables
    FROM information_schema.tables
    WHERE table_schema = 'public';

EOSQL

echo "Database initialization completed successfully"
echo "Summary:"
echo "  - Schema applied"
echo "  - Functions created"
echo "  - Node status initialized"
echo "  - Audit log entry created"
