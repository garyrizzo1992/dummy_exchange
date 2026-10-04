# KEDA scaling assessment

KEDA can use the existing private Prometheus service. It is not installed yet.
Observed load was approximately 3.4 API requests/second and 3.9 accepted
orders/second; capacity pressure has not been established. Traders call the
trading library directly, so their orders do not represent HTTP API demand.

| Target | Signal | Initial bounds | Assessment |
| --- | --- | --- | --- |
| API Deployment | Request rate; add in-flight requests and measure capacity | 1-3 | Simplest demonstration. Keep one replica for availability and observable demand. |
| Matching workers | New executable-backlog gauge and oldest executable-order age | 1-3 | Per-market advisory locks permit at most three simultaneous matching transactions across the three markets. |
| Trader StatefulSet | Scheduled or explicitly controlled simulated load | Fixed count initially | Supported, but these pods generate demand. Scaling them up in response to backlog increases pressure. |

Open-order count is unsuitable for matching workers: non-crossing limits
legitimately wait. Existing fill counters measure completed work, not demand.
An executable-backlog gauge must exclude unmatched limits and deduplicate
identical database observations across worker replicas.

The market-order delay came from insufficient simulated liquidity. More workers
cannot fix that. Market depth is now configurable in USD.

Implementation prerequisites:

1. Install a compatible KEDA release through a separate Argo CD application.
2. Use the private Prometheus endpoint, avoiding public Cloudflare Access.
   PromQL must return one scalar/vector sample.
3. Load-test to establish a capacity target; begin with API 1-3 replicas.
4. Let KEDA own replicas: omit fixed Helm replicas when enabled and configure
   Argo CD to ignore the target's replica count during synchronization.
5. API auth throttling is currently per process. Add shared or ingress throttling
   before API autoscaling so replicas do not multiply authentication allowances.
6. Budget PostgreSQL connections (API pool: 10 per replica), CPU and memory.
   Configure scale-down stabilization and missing-metric behavior. Pod scaling
   does not provision OCI nodes.

References: [Prometheus scaler](https://keda.sh/docs/2.18/scalers/prometheus/)
and [supported workloads](https://keda.sh/docs/2.18/concepts/scaling-deployments/).
