CREATE TABLE market_state (
  instrument text PRIMARY KEY REFERENCES instruments(symbol),
  reference_price numeric(30,10) NOT NULL,
  change_24h numeric(30,10) NOT NULL DEFAULT 0,
  updated_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE orders ADD COLUMN IF NOT EXISTS is_system boolean NOT NULL DEFAULT false;
CREATE INDEX IF NOT EXISTS orders_system_book_idx ON orders(instrument, is_system, status, side, limit_price, sequence);
INSERT INTO instruments(symbol,base_currency,quote_currency,tick_size) VALUES
 ('BTC-USD','BTC','USD',0.01),('ETH-USD','ETH','USD',0.01),('SOL-USD','SOL','USD',0.01)
ON CONFLICT DO NOTHING;
INSERT INTO market_state(instrument,reference_price) VALUES
 ('BTC-USD',65000),('ETH-USD',3500),('SOL-USD',150)
ON CONFLICT DO NOTHING;
