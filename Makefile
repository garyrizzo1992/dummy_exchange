.PHONY: build test lint run-api run-worker run-simulator migrate seed bench
build:
	cargo build --workspace
test:
	cargo test --workspace
lint:
	cargo clippy --workspace --all-targets -- -D warnings
run-api:
	cargo run -p exchange-api
run-worker:
	cargo run -p exchange-worker
run-simulator:
	cargo run -p exchange-simulator
migrate:
	sqlx migrate run --source migrations
seed:
	psql $$DATABASE_URL -f scripts/seed.sql
bench:
	k6 run benchmarks/orders.js
