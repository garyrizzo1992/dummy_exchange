# Autonomous traders and tracing

Matching workers remain a Deployment. The market simulator remains a singleton liquidity/reference-price provider. Autonomous traders are a separate StatefulSet using the simulator image with the `trader` argument.

Each StatefulSet name maps to one persistent user in `simulated_traders`. Startup funds that account once. Restarts, scale-down and scale-up reuse its identifier and balances; no pod PVC is required because PostgreSQL owns account state. A dedicated connection holds a PostgreSQL advisory lock for the trader's lifetime, preventing overlapping pods with the same identity from trading simultaneously. Losing that connection stops trading and requires ownership to be reacquired.

Traders place random buys/sells through the same acceptance code as human accounts, reserve funds, settle normally, and cancel their own unfilled orders after 30 seconds. They never bypass balances or top up a depleted account. The random seed is derived from trader identity; scheduling affects exact execution.

Configure `traders.replicas`, `traders.intervalMs` and `simulationSeed` in the exchange chart. The replica count has no artificial upper bound; actual throughput is bounded by node resources, database connections and matching capacity. Each trader uses one persistent database connection, and additional matching pods help distribute instruments rather than match one instrument concurrently.

Application pods include a resource-limited OpenTelemetry Collector sidecar when `tracing.enabled` is true. It accepts OTLP only on the pod loopback interface and exports to private Tempo. W3C context is saved on accepted orders and continued by settlement spans in matching workers. Grafana provisions the Tempo datasource, an Exchange Tracing dashboard, and Loki-to-trace links. Tempo retains local trace data for 72 hours on a PVC, appropriate for this development cluster.

Trader updates run in Argo CD sync wave 1, after the API and matching Deployments are healthy. Initial simulation-only legacy-matcher corrections are preserved in audit_log.

## Live references and profit rankings

Dev uses Coinbase BTC/USD, ETH/USD and SOL/USD ticker snapshots and 24-hour
statistics, fetched by the singleton simulator about every five seconds plus
request latency. Orders, order books, fills and balances remain simulated. The
chart shows this exchange's fills rather than Coinbase trades. Local/default
Helm values retain reproducible simulated prices via `priceSource: simulated`.

Live quotes must be positive and the last source trade no more than 120 seconds
old. Fetch failures retain the last reference; after 30 seconds without a
successful refresh, new orders pause and stale system liquidity is cancelled.
The UI displays source/freshness. `market_price_feed_errors_total` and
`market_price_feed_last_success_timestamp_seconds` expose feed health.

The leaderboard ranks total portfolio return percentage and displays profit in
USD. Available and reserved holdings count, and crypto uses reference prices.
This includes realized and unrealized profit. System liquidity is unranked.
New accounts capture their initial portfolio value once. Existing accounts begin
at a recorded migration snapshot; transitioning from simulated to Coinbase
prices resets that baseline once, atomically with all market updates, to prevent
invented price differences appearing as profit. The tracking timestamp is shown
on each row. Pod restarts do not reset balances or live-price baselines.
