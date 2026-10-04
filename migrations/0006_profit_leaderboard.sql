-- Existing accounts begin tracking at this snapshot; historical seed valuations
-- were not recorded. New accounts capture their initial value at creation.
ALTER TABLE market_state ADD COLUMN price_source text NOT NULL DEFAULT 'simulated';
ALTER TABLE market_state ADD COLUMN source_updated_at timestamptz;
ALTER TABLE users ADD COLUMN profit_baseline_source text NOT NULL DEFAULT 'simulated';
ALTER TABLE users ADD COLUMN initial_equity_usd numeric;
ALTER TABLE users ADD COLUMN profit_tracking_started_at timestamptz;
WITH prices AS (
    SELECT i.base_currency AS currency, s.reference_price AS price
    FROM instruments i JOIN market_state s ON s.instrument=i.symbol
    WHERE i.quote_currency='USD'
), totals AS (
    SELECT u.id, COALESCE(SUM((a.available+a.reserved) *
        CASE WHEN a.currency='USD' THEN 1 ELSE COALESCE(p.price,0) END),0) AS equity
    FROM users u LEFT JOIN accounts a ON a.user_id=u.id
    LEFT JOIN prices p ON p.currency=a.currency GROUP BY u.id
)
UPDATE users u SET initial_equity_usd=t.equity,profit_tracking_started_at=now()
FROM totals t WHERE t.id=u.id AND u.id <> '00000000-0000-0000-0000-000000000001'::uuid;
