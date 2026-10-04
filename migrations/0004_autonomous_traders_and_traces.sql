CREATE TABLE simulated_traders (
    trader_key text PRIMARY KEY,
    user_id uuid NOT NULL UNIQUE REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE orders ADD COLUMN trace_context jsonb NOT NULL DEFAULT '{}';
CREATE INDEX orders_user_open_idx ON orders(user_id, created_at) WHERE status IN ('open','partially_filled');
