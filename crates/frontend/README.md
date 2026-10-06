# Exchange frontend

The Rust/Leptos client is hosted on Cloudflare Pages at https://exchange.garyrizzo.dev
and calls https://api.garyrizzo.dev/v1. The API also serves its bundled frontend at
`/v1/ui/` for same-origin use.

The API Docker build compiles this crate to WebAssembly and packages wasm-bindgen assets.

## Standalone static artifact

With the WASM target and wasm-bindgen CLI installed as shown below, run from the repository root:

```sh
python3 scripts/build-frontend.py --api-origin https://api.garyrizzo.dev
```

The complete site is written to `target/frontend-static/`: `index.html`,
`style.css`, and the `pkg/` JavaScript/WebAssembly assets, with a ZIP archive at
`target/exchange-frontend-static.zip`. Serve that directory
with a static web server. Omit `--api-origin` when an API or reverse proxy serves
`/v1/` on the same origin. The API origin is compiled into the WASM; rebuild to
change it. The existing API Docker build continues using same-origin requests.

The application workflow uploads `exchange-frontend-static` as a separate GitHub
Actions artifact. On trusted main builds, the deployment job waits for the
Terraform-managed project and uploads the artifact using Wrangler. Pull requests
build artifacts without deploying. The API's development CORS policy allows the
custom frontend origin and production Pages origin, including JSON and bearer
Authorization headers and OPTIONS preflight requests. Preview origins are not
allowed automatically.

Cloudflare Pages projects and custom domains can be managed by Terraform using
`cloudflare_pages_project` and `cloudflare_pages_domain`, plus a DNS record.
Build and upload the site separately in CI using Wrangler; Terraform manages
the hosting configuration, while CI deploys the static files. Development uses
project `dummy-exchange-dev-frontend`. The Cloudflare API token must include
Account > Cloudflare Pages > Edit alongside the existing tunnel/DNS permissions.
The generated `_headers` file allows API connections in the content security
policy and asks browsers to revalidate assets between releases.

## Local API hosting

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
