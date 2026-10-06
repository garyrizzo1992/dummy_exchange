-- One-time operator action, never a migration or a startup task.
-- Stop traders and matching workers before running with psql -v ON_ERROR_STOP=1.
-- Rotating user IDs fences old Kafka commands while keeping StatefulSet identities.
BEGIN;
SET LOCAL lock_timeout = '15s';
SET LOCAL statement_timeout = '180s';
LOCK TABLE simulated_traders, users, accounts, orders, fills, outbox_events, audit_log IN SHARE ROW EXCLUSIVE MODE;
CREATE TEMP TABLE reset_bot_accounts ON COMMIT DROP AS
    SELECT trader_key, user_id AS old_id, gen_random_uuid() AS new_id FROM simulated_traders;
CREATE TEMP TABLE reset_bot_orders ON COMMIT DROP AS
    SELECT id FROM orders WHERE user_id IN (SELECT old_id FROM reset_bot_accounts);
CREATE UNIQUE INDEX ON reset_bot_orders(id);
-- A fill belongs to both counterparties. Retain human counterparties' history
-- rather than silently removing it; handle any such records explicitly.
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM fills f JOIN orders m ON m.id=f.maker_order_id JOIN orders t ON t.id=f.taker_order_id
        WHERE (m.id IN (SELECT id FROM reset_bot_orders) OR t.id IN (SELECT id FROM reset_bot_orders))
        AND (m.user_id NOT IN (SELECT old_id FROM reset_bot_accounts) AND NOT m.is_system
             OR t.user_id NOT IN (SELECT old_id FROM reset_bot_accounts) AND NOT t.is_system)
    ) THEN RAISE EXCEPTION 'Bot reset would remove a human counterparty trade'; END IF;
END $$;
DELETE FROM fills WHERE maker_order_id IN (SELECT id FROM reset_bot_orders) OR taker_order_id IN (SELECT id FROM reset_bot_orders);
DELETE FROM outbox_events WHERE aggregate_id IN (SELECT id FROM reset_bot_orders) OR aggregate_id IN (SELECT old_id FROM reset_bot_accounts);
DELETE FROM audit_log WHERE actor_id IN (SELECT old_id FROM reset_bot_accounts) OR entity_id IN (SELECT id FROM reset_bot_orders);
DELETE FROM orders WHERE id IN (SELECT id FROM reset_bot_orders);
INSERT INTO users(id,email,password_hash,initial_equity_usd,profit_tracking_started_at,profit_baseline_source)
    SELECT b.new_id, 'trader-' || b.new_id || '@exchange.internal', 'disabled',
           100000 + COALESCE((SELECT sum(s.reference_price * v.quantity)
               FROM (VALUES ('BTC-USD',1),('ETH-USD',10),('SOL-USD',100)) AS v(symbol,quantity)
               JOIN market_state s ON s.instrument=v.symbol),0), now(), 'simulated'
    FROM reset_bot_accounts b;
INSERT INTO accounts(user_id,currency,available,reserved)
    SELECT b.new_id, v.currency, v.amount, 0 FROM reset_bot_accounts b
    CROSS JOIN (VALUES ('USD',100000),('BTC',1),('ETH',10),('SOL',100)) AS v(currency,amount);
UPDATE simulated_traders t SET user_id=b.new_id,created_at=now(),last_seen_at=now()
    FROM reset_bot_accounts b WHERE t.trader_key=b.trader_key;
DELETE FROM accounts WHERE user_id IN (SELECT old_id FROM reset_bot_accounts);
DELETE FROM users WHERE id IN (SELECT old_id FROM reset_bot_accounts);
SELECT count(*) AS reset_bots FROM reset_bot_accounts;
COMMIT;
