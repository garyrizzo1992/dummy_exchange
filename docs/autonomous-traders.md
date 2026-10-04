# Autonomous traders and tracing

Matching workers remain a Deployment. The market simulator remains a singleton liquidity/reference-price provider. Autonomous traders are a separate StatefulSet using the simulator image with the `trader` argument.

Each StatefulSet name maps to one persistent user in `simulated_traders`. Startup funds that account once. Restarts, scale-down and scale-up reuse its identifier and balances; no pod PVC is required because PostgreSQL owns account state. A dedicated connection holds a PostgreSQL advisory lock for the trader's lifetime, preventing overlapping pods with the same identity from trading simultaneously. Losing that connection stops trading and requires ownership to be reacquired.

Traders place random buys/sells through the same acceptance code as human accounts, reserve funds, settle normally, and cancel their own unfilled orders after 30 seconds. They never bypass balances or top up a depleted account. The random seed is derived from trader identity; scheduling affects exact execution.

Configure `traders.replicas`, `traders.intervalMs` and `simulationSeed` in the exchange chart. The replica count has no artificial upper bound; actual throughput is bounded by node resources, database connections and matching capacity. Each trader uses one persistent database connection, and additional matching pods help distribute instruments rather than match one instrument concurrently.

Application pods include a resource-limited OpenTelemetry Collector sidecar when `tracing.enabled` is true. It accepts OTLP only on the pod loopback interface and exports to private Tempo. W3C context is saved on accepted orders and continued by settlement spans in matching workers. Grafana provisions the Tempo datasource, an Exchange Tracing dashboard, and Loki-to-trace links. Tempo retains local trace data for 72 hours on a PVC, appropriate for this development cluster.
