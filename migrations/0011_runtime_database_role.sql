-- Authentication credentials are provisioned independently through Vault.
-- This non-login role holds DML rights, never ownership or migration rights.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'exchange_runtime') THEN
        CREATE ROLE exchange_runtime NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
            NOREPLICATION NOBYPASSRLS;
    END IF;
END
$$;

DO $$
BEGIN
    EXECUTE format('GRANT CONNECT ON DATABASE %I TO exchange_runtime', current_database());
END
$$;
GRANT USAGE ON SCHEMA public TO exchange_runtime;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON
    users, accounts, instruments, orders, fills, outbox_events,
    audit_log, market_state, simulated_traders
    TO exchange_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO exchange_runtime;
-- Grant new application objects deliberately in each future migration.
-- Do not grant access to _sqlx_migrations or database/schema ownership.

REVOKE ALL ON _sqlx_migrations FROM exchange_runtime;
