-- Bound account trade-count lookups to the account's own order/fill indexes.
CREATE INDEX IF NOT EXISTS orders_user_id_idx ON orders(user_id);
CREATE INDEX IF NOT EXISTS fills_taker_order_id_idx ON fills(taker_order_id);
