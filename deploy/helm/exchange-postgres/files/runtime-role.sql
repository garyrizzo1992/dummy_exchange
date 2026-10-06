\set ON_ERROR_STOP on
\getenv runtime_password RUNTIME_PASSWORD
\getenv monitor_password MONITOR_PASSWORD
BEGIN;
SET LOCAL log_statement = 'none';
SELECT 1 / CASE WHEN length(:'runtime_password') >= 32 THEN 1 ELSE 0 END AS password_length_check;
SELECT 'CREATE ROLE exchange_runtime NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS'
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'exchange_runtime')
\gexec
SELECT 'CREATE ROLE exchange_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS'
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'exchange_app')
\gexec
SELECT format('ALTER ROLE exchange_app PASSWORD %L', :'runtime_password')
\gexec
SELECT 1 / CASE WHEN length(:'monitor_password') >= 32 THEN 1 ELSE 0 END AS monitor_password_length_check;
SELECT 'CREATE ROLE exchange_monitor LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS'
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'exchange_monitor')
\gexec
SELECT format('ALTER ROLE exchange_monitor PASSWORD %L', :'monitor_password')
\gexec
ALTER ROLE exchange_runtime NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
ALTER ROLE exchange_app NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
ALTER ROLE exchange_monitor NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
GRANT pg_monitor TO exchange_monitor;
GRANT exchange_runtime TO exchange_app;
SELECT format('GRANT CONNECT ON DATABASE %I TO exchange_runtime', current_database())
\gexec
GRANT USAGE ON SCHEMA public TO exchange_runtime;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
SELECT format('GRANT SELECT,INSERT,UPDATE,DELETE ON TABLE %I.%I TO exchange_runtime', schemaname,tablename)
FROM pg_tables WHERE schemaname='public' AND tablename <> '_sqlx_migrations'
\gexec
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO exchange_runtime;
-- Tables created by the administrator in subsequent migrations inherit DML.
-- The migration explicitly revokes the migration-history exception.
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO exchange_runtime;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE,SELECT ON SEQUENCES TO exchange_runtime;
COMMIT;
