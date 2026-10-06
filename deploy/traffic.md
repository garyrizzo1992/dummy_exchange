# Traffic flow

These diagrams describe the development deployment. Arrows show who starts a request or sends data; replies return over the same connection. Dashed arrows represent control or deployment traffic.

## Browser and trading traffic

```mermaid
flowchart TB
    Browser["Browser<br/>Rust / WebAssembly frontend"]
    subgraph Cloudflare["Cloudflare"]
        Pages["Pages<br/>exchange.garyrizzo.dev<br/>HTML, JavaScript, WASM and CSS"]
        Edge["API hostname<br/>api.garyrizzo.dev"]
    end
    subgraph OCI["OCI private network"]
        subgraph K8s["Kubernetes ? control-plane and worker nodes"]
            Tunnel["cloudflared ? 2 replicas<br/>Outbound tunnel connections"]
            API["Rust / Axum API<br/>ClusterIP service ? HTTP 3000<br/>Authentication, orders and market data"]
            Bots["Autonomous traders<br/>One persistent account per bot<br/>One PostgreSQL session per active bot"]
            Kafka["Kafka ? 3 brokers<br/>Internal listener ? TCP 9092<br/>exchange.order-commands"]
            Workers["Rust workers<br/>Consume bot commands<br/>Match orders by price and time"]
            Generator["Market generator<br/>Independent price ticks<br/>System buy / sell liquidity"]
            PG[("PostgreSQL ? TCP 5432<br/>Users and balances<br/>Orders, fills and audit / outbox rows<br/>Saved generator price, seed and step")]
            Disk[("Persistent volume<br/>PostgreSQL data")]
        end
    end
    Browser -->|"HTTPS 443 ? load static frontend"| Pages
    Browser -->|"HTTPS 443 ? /v1/* JSON requests<br/>CORS; JWT for account actions"| Edge
    Tunnel -.->|"Establish outbound encrypted tunnel"| Edge
    Edge -->|"Route API requests over tunnel"| Tunnel
    Tunnel -->|"HTTP ? /v1/* only"| API
    API -->|"SQL ? read markets, books, trades and leaderboard<br/>Create / cancel user orders and reserve balances"| PG
    Bots -->|"SQL ? read prices and balances<br/>Account ownership lock and heartbeat"| PG
    Bots -->|"Publish place / cancel commands<br/>Account ID as partition key"| Kafka
    Workers -->|"Consume commands; commit offsets after DB success"| Kafka
    Workers -->|"SQL transactions ? apply commands<br/>Lock each instrument, match orders<br/>Write fills and settle balances"| PG
    Generator -->|"SQL transactions ? save next price and step<br/>Replace system liquidity orders"| PG
    PG -->|"Persist database files"| Disk
```

The browser polls the API about every two seconds. Human orders are written directly by the API; bot orders use Kafka. Workers match both from PostgreSQL. Generated prices continue from saved state after a restart. Unmatched tunnel routes return 404.

## Monitoring and scaling traffic

```mermaid
flowchart TB
    Operator["Operator browser"]
    CF["Cloudflare service hostnames<br/>Grafana, Argo CD and Prometheus<br/>Access email login protects Prometheus"]
    Tunnel["cloudflared"]
    subgraph Cluster["Private Kubernetes services"]
        Grafana["Grafana ? HTTP 3000<br/>Dashboards, logs and traces"]
        Argo["Argo CD ? HTTPS 443<br/>Deployment status and controls"]
        Prom["Prometheus ? HTTP 9090"]
        Metrics["Metrics endpoints<br/>API :3000 ? workers :3001<br/>Generator :3002 ? traders :3003<br/>Load controller :3004<br/>Node exporter and kube-state-metrics"]
        Load["Load controller<br/>Publishes target trader count"]
        KEDA["KEDA / HPA"]
        Kafka["Kafka<br/>Worker consumer-group lag"]
        Scale["Trader StatefulSet<br/>Worker Deployment"]
        Services["API, workers and generator"]
        Collectors["Local OpenTelemetry sidecars<br/>OTLP HTTP ? 127.0.0.1:4318"]
        Traders["Traders<br/>Direct trace export"]
        Tempo["Tempo<br/>OTLP gRPC :4317 / HTTP :4318<br/>Trace queries :3200"]
        Alloy["Grafana Alloy"]
        KubeAPI["Kubernetes API<br/>Pod discovery and log streams"]
        Loki["Loki ? HTTP 3100<br/>Stored container logs"]
    end
    Operator -->|"HTTPS 443"| CF
    CF -->|"Existing outbound tunnel"| Tunnel
    Tunnel -->|"HTTP"| Grafana
    Tunnel -->|"HTTPS"| Argo
    Tunnel -->|"HTTP ? after Access login"| Prom
    Prom -->|"HTTP scrapes"| Metrics
    Load -->|"Expose simulation_target_traders"| Metrics
    KEDA -.->|"Query trader target"| Prom
    KEDA -.->|"Read consumer lag ? TCP 9092"| Kafka
    KEDA -.->|"Set replica counts through Kubernetes API"| Scale
    Services -->|"OTLP HTTP"| Collectors
    Collectors -->|"OTLP gRPC ? 4317"| Tempo
    Traders -->|"OTLP HTTP ? 4318"| Tempo
    Alloy -->|"Discover pods and read logs"| KubeAPI
    Alloy -->|"Push logs"| Loki
    Grafana -->|"PromQL queries"| Prom
    Grafana -->|"LogQL queries"| Loki
    Grafana -->|"Trace searches"| Tempo
```

Trader scaling currently stays between 5 and 8 replicas; workers between 1 and 2. The load controller chooses a trader target, while worker scaling follows Kafka lag. Bot accounts remain in PostgreSQL when their pods stop.

## Build, deployment and infrastructure traffic

```mermaid
flowchart TB
    Dev["Developer"]
    Git["GitHub repository<br/>Source, Helm charts and image digests"]
    CI["GitHub Actions<br/>Rust checks and integration tests<br/>PostgreSQL / Kafka test fixtures"]
    Images["Native amd64 / arm64 builds"]
    Registry["Docker Hub<br/>Immutable image digests"]
    Frontend["Static frontend build<br/>HTML / JS / WASM artifact"]
    Pages["Cloudflare Pages<br/>Production frontend"]
    Promotion["Promotion job<br/>Commit tested image digests"]
    Argo["Argo CD<br/>Watch main and render Helm charts"]
    KubeAPI["Kubernetes API"]
    Migration["API migration job<br/>Runs before application rollout"]
    PG[("PostgreSQL")]
    Pods["Application pods<br/>Nodes pull container images"]
    Infra["Trusted OCI CI runner<br/>Terraform and Ansible"]
    OCI["OCI APIs<br/>Compute, networking, Bastion and Vault"]
    CF["Cloudflare APIs<br/>DNS, Tunnel, Pages project and Access"]
    Nodes["Private Kubernetes nodes"]
    Secrets["External Secrets controller"]
    KSecrets["Kubernetes Secret<br/>Database credentials from Vault"]
    Dev -->|"Git push / pull request"| Git
    Git -.->|"Trigger workflows"| CI
    CI --> Images
    Images -->|"HTTPS ? push images"| Registry
    CI --> Frontend
    Frontend -->|"HTTPS ? Wrangler direct upload"| Pages
    Registry -->|"Published digests"| Promotion
    Promotion -->|"Git commit"| Git
    Argo -->|"HTTPS ? fetch main"| Git
    Argo -.->|"Apply rendered resources"| KubeAPI
    KubeAPI -.-> Migration
    Migration -->|"SQL ? schema migrations"| PG
    KubeAPI -.->|"Roll out workloads"| Pods
    Pods -->|"HTTPS ? pull images by digest"| Registry
    Git -.->|"Infrastructure workflow"| Infra
    Infra -->|"HTTPS ? provision resources"| OCI
    Infra -->|"HTTPS ? configure routing and hosting"| CF
    Infra -->|"Private SSH ? Ansible bootstrap"| Nodes
    Secrets -->|"HTTPS ? retrieve secrets from OCI Vault"| OCI
    Secrets -.->|"Synchronize credentials"| KSecrets
    KSecrets -.->|"Inject referenced credentials"| Pods
```

Terraform manages the Pages project and domain; the frontend workflow uploads the built assets. Argo CD deploys backend image digests from Git. Database migrations run before the application rollout.
