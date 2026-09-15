INSERT INTO instruments(symbol, base_currency, quote_currency, tick_size) VALUES ('BTC-USD','BTC','USD',0.01) ON CONFLICT DO NOTHING;
