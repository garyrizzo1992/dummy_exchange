"""Persistent market and bot reset checks on disposable exchange-verification-postgres.
Run after verify-exchange.py, with all of its service processes stopped.
"""
import os, subprocess, uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('EXCHANGE_BIN_DIR', ROOT / 'target/debug'))
ENV = dict(os.environ, PGHOST='127.0.0.1', PGPORT='25432', PGUSER='postgres',
           PGDATABASE='dummy_exchange', PGPASSWORD='test-only-private-password', PGSSLMODE='disable',
           PRICE_SOURCE='simulated', SIMULATION_SEED='42', SIMULATOR_METRICS_BIND='127.0.0.1:23019',
           OTEL_EXPORTER_OTLP_ENDPOINT='http://127.0.0.1:4318')

def sql(query):
    return subprocess.check_output(['docker', 'exec', 'exchange-verification-postgres', 'psql',
        '-U', 'postgres', '-d', 'dummy_exchange', '-v', 'ON_ERROR_STOP=1', '-Atc', query], text=True).strip()

def tick(seed='42'):
    subprocess.run([str(BIN / ('exchange-simulator.exe' if os.name == 'nt' else 'exchange-simulator')), 'generate-once'],
        env=dict(ENV, SIMULATION_SEED=seed), check=True)

# Initialize from the last saved valuation, even if it came from Coinbase.
sql("UPDATE market_state SET generator_anchor_price=NULL,generator_seed=NULL,generator_step=0,reference_price=12345.67,price_source='coinbase'")
tick()
first = sql("SELECT generator_seed FROM market_state WHERE instrument='BTC-USD'")
assert sql("SELECT count(*) FROM market_state WHERE generator_step=1 AND generator_anchor_price=12345.67 AND price_source='simulated' AND source_updated_at IS NULL") == '3'
# A new process and a different configured seed must use the saved generator seed.
tick('999')
assert sql("SELECT generator_seed FROM market_state WHERE instrument='BTC-USD'") == first
assert sql("SELECT count(*) FROM market_state WHERE generator_step=2 AND generator_anchor_price=12345.67") == '3'
assert sql("SELECT count(*) FROM orders WHERE is_system AND status='open'") == '12'
old = sql("SELECT string_agg(user_id::text,',' ORDER BY trader_key) FROM simulated_traders")
assert old, 'The preceding exchange verification must create a bot'
human_before = sql("SELECT md5(string_agg(row_to_json(a)::text,',' ORDER BY a.id)) FROM accounts a WHERE user_id NOT IN (SELECT user_id FROM simulated_traders)")
# A bot/human fill must prevent a destructive reset, with full rollback.
bot = old.split(',')[0]
human = sql("SELECT id FROM users WHERE email='integration@example.com'")
maker, taker, fill = (str(uuid.uuid4()) for _ in range(3))
sql(f"INSERT INTO orders(id,user_id,client_order_id,instrument,side,order_type,quantity,remaining,limit_price,status) VALUES ('{maker}','{bot}','reset-guard-bot','BTC-USD','buy','limit',1,0,1,'filled'),('{taker}','{human}','reset-guard-human','BTC-USD','sell','limit',1,0,1,'filled'); INSERT INTO fills(id,maker_order_id,taker_order_id,instrument,price,quantity) VALUES('{fill}','{maker}','{taker}','BTC-USD',1,1)")
with (ROOT / 'scripts/reset-bots.sql').open() as source:
    refused = subprocess.run(['docker','exec','-i','exchange-verification-postgres','psql','-U','postgres','-d','dummy_exchange',
                             '-v','ON_ERROR_STOP=1'],stdin=source,capture_output=True,text=True)
assert refused.returncode != 0 and 'human counterparty trade' in refused.stderr
assert sql("SELECT string_agg(user_id::text,',' ORDER BY trader_key) FROM simulated_traders") == old
assert sql(f"SELECT count(*) FROM fills WHERE id='{fill}'") == '1'
sql(f"DELETE FROM fills WHERE id='{fill}'; DELETE FROM orders WHERE id IN ('{maker}','{taker}')")
with (ROOT / 'scripts/reset-bots.sql').open() as source:
    subprocess.run(['docker','exec','-i','exchange-verification-postgres','psql','-U','postgres','-d','dummy_exchange',
                    '-v','ON_ERROR_STOP=1'],stdin=source,check=True)
assert sql("SELECT string_agg(user_id::text,',' ORDER BY trader_key) FROM simulated_traders") != old
assert sql("SELECT count(*) FROM orders WHERE user_id IN (SELECT user_id FROM simulated_traders)") == '0'
assert sql("SELECT count(*) FROM accounts a JOIN simulated_traders t ON t.user_id=a.user_id WHERE reserved<>0 OR available<>CASE currency WHEN 'USD' THEN 100000 WHEN 'BTC' THEN 1 WHEN 'ETH' THEN 10 WHEN 'SOL' THEN 100 END") == '0'
assert human_before == sql("SELECT md5(string_agg(row_to_json(a)::text,',' ORDER BY a.id)) FROM accounts a WHERE user_id NOT IN (SELECT user_id FROM simulated_traders)")
print('Passed: persisted generation across process restarts, saved seed, independent prices, bot identity rotation, fresh funding and unchanged human balances.')
