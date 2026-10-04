# Exchange frontend

The Rust/Leptos client is served by the API at `/v1/ui/`, using the same origin for authentication and market requests. Dev URL: https://api.garyrizzo.dev/v1/ui/.

The API Docker build compiles this crate to WebAssembly and packages wasm-bindgen assets. For local use:

```sh
rustup target add wasm32-unknown-unknown
cargo build -p exchange-frontend --target wasm32-unknown-unknown --release --locked
cargo install wasm-bindgen-cli --version 0.2.128 --locked
wasm-bindgen --target web --out-name exchange_frontend --out-dir crates/frontend/dist/pkg target/wasm32-unknown-unknown/release/exchange-frontend.wasm
cargo run -p exchange-api
```

Set database settings and JWT_SECRET before starting the API. Open http://127.0.0.1:3000/v1/ui/ from the repository root. Tokens exist only in memory; reload signs you out. Public and authenticated data refresh every two seconds without overlapping requests. The chart shows the last 60 executed prices rather than fabricated candles.

Quantity shortcuts use 25%, 50%, 75% or all available funds. Buys divide available
USD by the entered limit price, or by 105% of the live reference for market orders;
sells use available base currency. Quantities round down to eight decimals and
respect the one-million-unit order limit. Reserved funds are excluded. The API
rechecks funds when accepting the order. Cancel all orders applies across markets
only to the signed-in account and releases remaining reservations atomically.
