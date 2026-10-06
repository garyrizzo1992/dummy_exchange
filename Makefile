.PHONY: build test lint run-api run-worker run-simulator migrate bench
build:
	cargo build --workspace --locked
test:
	cargo test --workspace --locked
lint:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
run-api:
	cargo run -p exchange-api
run-worker:
	cargo run -p exchange-worker
run-simulator:
	cargo run -p exchange-simulator
migrate:
	cargo run -p exchange-api --locked -- migrate
bench:
	k6 run benchmarks/orders.js
