"""Verify runtime grants in a disposable database on the development PostgreSQL pod."""
from pathlib import Path
import subprocess
import uuid

ROOT = Path(__file__).resolve().parents[1]
suffix = uuid.uuid4().hex[:12]
database = "security_role_" + suffix
role = "security_runtime_" + suffix
login = "security_app_" + suffix
monitor = "security_monitor_" + suffix
# Each test uses its own database and cluster role. No existing rows are touched.
sql = f"CREATE DATABASE {database};\n\\connect {database}\nCREATE TABLE _sqlx_migrations (version bigint);\n"
provisioning = (ROOT / "deploy/helm/exchange-postgres/files/runtime-role.sql").read_text()
provisioning = provisioning.replace("exchange_runtime", role).replace("exchange_app", login).replace("exchange_monitor", monitor)
provisioning = provisioning.replace("\\getenv runtime_password RUNTIME_PASSWORD", "\\set runtime_password test-only-runtime-password-at-least-32-bytes")
provisioning = provisioning.replace("\\getenv monitor_password MONITOR_PASSWORD", "\\set monitor_password test-only-monitor-password-at-least-32-bytes")
sql += provisioning + "\n"
for path in sorted((ROOT / "migrations").glob("*.sql")):
    sql += "BEGIN;\n" + path.read_text().replace("exchange_runtime", role) + "\nCOMMIT;\n"
sql += f"""
SET ROLE {login};
DO $$
BEGIN
    IF NOT has_schema_privilege(current_user, 'public', 'USAGE') THEN
        RAISE EXCEPTION 'Runtime schema usage is missing';
    END IF;
    IF has_schema_privilege(current_user, 'public', 'CREATE') THEN
        RAISE EXCEPTION 'Runtime must not create schema objects';
    END IF;
    IF has_table_privilege(current_user, '_sqlx_migrations', 'SELECT') THEN
        RAISE EXCEPTION 'Runtime must not access migration history';
    END IF;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname=current_user
               AND (rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls)) THEN
        RAISE EXCEPTION 'Runtime has administrative privileges';
    END IF;
END
$$;
-- Registration and account funding, followed by generator/trader bookkeeping.
INSERT INTO users (email,password_hash) VALUES ('fixture@example.invalid','fixture-only');
INSERT INTO accounts (user_id,currency,available)
    SELECT id,'USD',1000 FROM users WHERE email='fixture@example.invalid';
UPDATE accounts SET available=available-10,reserved=reserved+10;
INSERT INTO simulated_traders (trader_key,user_id) SELECT 'fixture',id FROM users WHERE email='fixture@example.invalid';
UPDATE market_state SET reference_price=reference_price+1;
INSERT INTO audit_log (action) VALUES ('fixture');
-- Sequence-backed writes and cleanup require their separate grants.
INSERT INTO orders (user_id,client_order_id,instrument,side,order_type,quantity,remaining,limit_price,status)
    SELECT id,'fixture','BTC-USD','buy','limit',1,1,10,'open'
    FROM users WHERE email='fixture@example.invalid';
UPDATE orders SET status='cancelled',remaining=0;
DELETE FROM orders;
RESET ROLE;
SET ROLE {monitor};
DO $$
BEGIN
    IF has_table_privilege(current_user, 'users', 'SELECT') THEN
        RAISE EXCEPTION 'Monitor must not read application users';
    END IF;
    IF NOT pg_has_role(current_user, 'pg_monitor', 'MEMBER') THEN
        RAISE EXCEPTION 'Monitor statistics access is missing';
    END IF;
END
$$;
SELECT count(*) FROM pg_stat_database;
RESET ROLE;
"""
try:
    result = subprocess.run(
        ["bash", "provisioning/ansible/dev-kubectl.sh", "exec", "-i", "-n", "dummy-exchange",
         "dummy-exchange-dev-dummy-exchange-postgres-0", "--", "psql", "-U", "postgres",
         "-d", "postgres", "-v", "ON_ERROR_STOP=1", "-q"],
        input=sql, text=True, cwd=ROOT, capture_output=True,
    )
    if result.returncode:
        raise RuntimeError(result.stderr)
    print("Disposable runtime-role DML and privilege checks passed.")
finally:
    cleanup = subprocess.run(
        ["bash", "provisioning/ansible/dev-kubectl.sh", "exec", "-i", "-n", "dummy-exchange",
         "dummy-exchange-dev-dummy-exchange-postgres-0", "--", "psql", "-U", "postgres",
         "-d", "postgres", "-v", "ON_ERROR_STOP=1", "-q"],
        input=f"DROP DATABASE IF EXISTS {database} WITH (FORCE);\nDROP ROLE IF EXISTS {login};\nDROP ROLE IF EXISTS {monitor};\nDROP ROLE IF EXISTS {role};\n",
        text=True, cwd=ROOT, capture_output=True,
    )
    if cleanup.returncode:
        raise RuntimeError("Disposable fixture cleanup failed: " + cleanup.stderr)
