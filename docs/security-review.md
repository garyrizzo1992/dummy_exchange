# Security review

Reviewed on 6 October 2026 against commit `1b36b76` and the live development deployment.

The basic API protections held up in these tests, but there are changes to make before promoting the demo widely. The main ones are old TLS support, vulnerable components, database permissions and deployment safeguards.

## Follow-up on 7 October 2026

Remediation is tracked separately in [security-follow-up.md](security-follow-up.md) and [the rollout runbook](security-rollout.md). Live checks now reject TLS 1.0/1.1, accept TLS 1.2 and return API/frontend 200 responses with short-lived HSTS. Grafana and Argo redirect unauthenticated requests to Cloudflare Access. Unrelated pods cannot connect to API/PostgreSQL, while restricted operator pods can. A private off-node dump was downloaded, checksum-verified and restored into a disposable database in 16 seconds; the fixture was removed. These findings update the corresponding observations below; the original audit remains historical evidence.

The patched local dependency graph has no active MySQL/RSA dependency, and the high/critical dependency scan passed with narrowly scoped, expiring exceptions. Runtime-role tests and Kubernetes schema checks passed. Application image replacement scans, remaining third-party component advisories, operator Access login and application rollback rehearsal still need their own evidence. Do not read the original running-image inventory as a scan of the replacement application images.

## Scope

This is a focused review of ten common attack areas, guided by [OWASP Top 10](https://top10.owasp.org/2025/). It is not a claim to have tested every vulnerability or a ranking of attack frequency for this project.

Checks included public HTTPS requests, a disposable database and API pod using the deployed image, live Kubernetes configuration, Cloudflare settings, GitHub settings, Git history and dependency/image scans. The test database and pod were removed. Existing bot accounts and trades were not modified. No destructive CVE exploits, traffic floods, host writes or credential brute-forcing were used.

## Ten attack areas

| # | Attack area | What was tested | Result |
| --- | --- | --- | --- |
| 1 | Vulnerable components | OSV checked 427 locked Rust dependencies; Trivy scanned 32 running container images; vendor advisories were compared with configured tools. | **Needs work.** Known vulnerable versions and bundled packages remain. |
| 2 | Authentication bypass | Unsigned JWTs publicly; expired, missing/string expiration, wrong signatures and wrong algorithms in the disposable API. Weak registration passwords were also checked. | **Passed these checks.** Invalid tokens returned 401; short passwords returned 400. |
| 3 | Cross-account access | Two disposable users: user B tried to list and cancel user A's order. | **Passed.** B saw no A orders and cancellation returned 404; A could cancel it. |
| 4 | Injection and unsafe input | SQL-like login input, negative/zero/overflowing quantities, an attempted file traversal, plus bound-query review. | **Passed these checks.** Login returned 401, invalid quantities 400 and traversal 400. |
| 5 | Resource abuse | A 17 KB login body and 12 bounded login attempts, following two earlier auth probes. | **Passed basic limits.** Oversized input returned 413; throttling returned 429 with Retry-After. This does not test a distributed denial-of-service attack. |
| 6 | Browser security | Frontend/API CSP and frame headers; CORS preflights from the real frontend, a hostile origin, a lookalike domain and null origin. | **Passed checked controls.** Only the configured frontend origin was allowed by the API. This was not a full stored-XSS test. |
| 7 | Encryption | Real TLS 1.0, 1.1 and 1.2 handshakes; Cloudflare settings; PostgreSQL SSL setting and Kafka listener configuration. | **Needs work.** Legacy TLS handshakes succeeded, HSTS is off, and database/Kafka traffic is plaintext inside the cluster. |
| 8 | Exposed management endpoints | Unauthenticated Prometheus queries, Grafana search and Argo application requests; API metrics, readiness and .env paths. | **Basic access checks passed.** Prometheus redirected to Access; Grafana/Argo returned 401; private API paths returned 404. Grafana and Argo login surfaces remain public without a Cloudflare Access layer. |
| 9 | Excess privileges and lateral movement | Database role, service-account RBAC, pod settings, connections from an unrelated pod and a privileged-pod admission dry run. | **Needs work.** Runtime database access is superuser; the test pod reached API/PostgreSQL; privileged pod admission was allowed. Kafka blocked the unrelated pod, and the default service account could not read secrets or create pods. |
| 10 | Secrets and supply chain | Gitleaks on 101 commits; workflow permissions, action pinning, download verification, branch protection and infrastructure runner IAM. | **Mixed.** No secrets found. Actions use mutable tags, required PR/test checks are absent, and CI has no secret/dependency/image scanning gate. The infrastructure runner can manage all resources in the project compartment. |

## Versions and setup

Runtime versions below came from Kubernetes pod specifications and node information. Image scans use the running image digests. CI/provisioning versions come from committed configuration unless otherwise stated.

| Tool | Version | Setup |
| --- | --- | --- |
| Kubernetes / containerd | 1.37.1 / 2.3.6 | Two private OCI nodes; Oracle Linux 9.8, kernel 6.12.0-206.104.4.4.el9uek. |
| Calico / CoreDNS / etcd | 3.32.2 / 1.14.6 / 3.7.0-0 | Cluster networking, DNS and control-plane storage. |
| Argo CD / Dex / Redis | 3.5.3 / 2.45.1 / 8.2.3 | GitOps controllers and public Argo login through Tunnel. |
| Helm | 3.17.3 in CI/bootstrap; 4.2.1 inside Argo repo-server | CI validates local charts; Argo renders deployments. The older CLI needs patching. |
| Terraform / Ansible Core | 1.16.4 / 2.20.1 configured | Private OCI runner; instance-principal credentials. OCI CLI is pinned to 3.94.1. |
| Terraform providers | OCI 9.3.0; Cloudflare 5.24.0; HTTP 3.6.2; Ansible 1.5.0 | Provider lockfile includes checksums. |
| External Secrets | 2.11.0 | Reads OCI Vault through worker instance identity. |
| KEDA | 2.21.0 | Scales traders from Prometheus targets and workers from Kafka lag. |
| Kafka / Strimzi | 4.3.1 / 1.2.0 | Three brokers on two nodes; plaintext internal listener with network policies. |
| PostgreSQL | 18.6 | Confirmed by SQL; runtime uses postgres superuser; SCRAM password encryption, SSL off. |
| Local Path Provisioner | 0.0.32 | Node-local persistent volumes; affected by published advisories. |
| cloudflared | 2026.9.3 | Two non-root connectors with no service-account token; Cloudflare minimum TLS is 1.0. |
| Cloudflare Pages / Access | Managed services | Static frontend; Access email policy protects Prometheus. No customer-pinned server version. |
| Prometheus / Grafana | 3.14.0 / 13.2.0 | Internal metrics collection; dashboards exposed through Tunnel. |
| Loki / Alloy / Tempo | 3.7.8 / 1.20.1 / 2.10.3 | Private log/trace services; node-local storage. |
| OpenTelemetry Collector | 0.147.0 | API/worker/generator sidecars; traders export directly to Tempo. |
| Node / Kubernetes / PostgreSQL exporters | 1.12.1 / 2.20.0 / 0.20.1 | Private scrape targets. |
| Rust / wasm-bindgen / Wrangler | 1.98.1 / 0.2.128 / 4.147.0 configured | Locked Rust builds; static frontend upload from Actions using Node 22. |

## Findings to prioritise

1. **Disable legacy TLS.** Set Cloudflare minimum TLS to 1.2 in Terraform, enable HSTS after checking the affected domains, and repeat the handshake tests.
2. **Update the storage controller.** Local Path Provisioner 0.0.32 is affected by [path traversal](https://github.com/rancher/local-path-provisioner/security/advisories/GHSA-jr3w-9vfr-c746) and [helper-pod template injection](https://github.com/rancher/local-path-provisioner/security/advisories/GHSA-7fxv-8wr2-mfc4). The latter is fixed in 0.0.36. These need Kubernetes permissions to exploit; host-file attacks were not executed.
3. **Patch dependencies and images.** Upgrade vulnerable Rust libraries, the Helm CLI and bundled OS/Go/Java dependencies, then rescan the actual replacement digests.
4. **Reduce the impact of a compromised pod.** Split migration/runtime database roles, add API/database network policies, disable unnecessary service-account token mounts and enforce suitable pod admission rules. PostgreSQL currently permits root container execution; an exec identity check returned UID 0. That does not mean the PostgreSQL server process itself runs as root.
5. **Strengthen deployment controls.** Pin Actions to commit SHAs, require successful tests before release, and add secret/dependency/image scans. Protect Argo/Grafana through Access and narrow the infrastructure runner's permissions where practical.

The API trusts Cloudflare's client-IP header when configured. Public attempts to supply that header were blocked by Cloudflare, but direct private access has a weaker trust boundary while API network isolation is missing. Rate limits are per process/IP, not a shared account-level control.

## Advisory triage

- `jsonwebtoken` 9.3.1 is covered by [CVE-2026-25537](https://github.com/Keats/jsonwebtoken/security/advisories/GHSA-h395-gr6q-cpjc), fixed in 10.3.0. The current API requires typed expiration and does not use not-before restrictions; the tested malformed-expiration bypass failed. The dependency should still be upgraded.
- `opentelemetry_sdk` 0.31.0 is covered by [the baggage allocation advisory](https://github.com/open-telemetry/opentelemetry-rust/security/advisories/GHSA-w9wp-h8wv-79jx), fixed in 0.32.1. This project uses TraceContextPropagator, not BaggagePropagator, so that vulnerable path was not found in the current code.
- `rsa` 0.9.10 has [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html), with no published patch. It enters through sqlx-mysql; the application uses PostgreSQL and HS256 tokens. Remove unused MySQL features and confirm the dependency disappears.
- `paste` and `proc-macro-error2` are maintenance notices, not two demonstrated runtime exploits.
- Helm 3.17.3 is in the affected range for [chart extraction traversal](https://github.com/helm/helm/security/advisories/GHSA-hr2v-4r36-88hr); the v3 fix is 3.20.2. Do not feed exploit charts to the CI runner to demonstrate it. Argo's bundled Helm is separately versioned at 4.2.1.
- Application images include perl-base 5.36.0-7+deb12u3; scans report fixed security packages at +deb12u4. [Debian's tracker](https://security-tracker.debian.org/tracker/CVE-2026-13221) records the update. No public API path executing attacker-controlled Perl was demonstrated.
- Tempo and collector images include gRPC-Go below 1.79.3. [CVE-2026-33186](https://github.com/grpc/grpc-go/security/advisories/GHSA-p77j-4mvh-x3m3) requires a particular path-based authorization configuration; finding the package alone does not prove bypass here.
- Trivy reports CVE-2023-45853 for Debian zlib. [Debian explains](https://security-tracker.debian.org/tracker/CVE-2023-45853) that the vulnerable MiniZip code is not built into the Bookworm binary package. Treat this specific result as non-applicable, rather than counting it as a confirmed critical exploit.

## Image scan results

All 32 running images were scanned by digest. The scanner reported 181 distinct high/critical advisory IDs, including 11 critical IDs, before applicability review. Counts below are unique advisory IDs per image and include the non-applicable zlib result discussed above. They should not be read as counts of proven exploits.

| Running image | High IDs | Critical IDs |
| --- | ---: | ---: |
| `cloudflare/cloudflared:2026.9.3` | 2 | 0 |
| `docker.io/garyrizzo1992/dummy-exchange-api` | 19 | 4 |
| `docker.io/garyrizzo1992/dummy-exchange-simulator` | 19 | 4 |
| `docker.io/garyrizzo1992/dummy-exchange-worker` | 19 | 4 |
| `ghcr.io/dexidp/dex:v2.45.1` | 56 | 3 |
| `ghcr.io/external-secrets/external-secrets:v2.11.0` | 0 | 0 |
| `ghcr.io/kedacore/keda-admission-webhooks:2.21.0` | 0 | 0 |
| `ghcr.io/kedacore/keda-metrics-apiserver:2.21.0` | 0 | 0 |
| `ghcr.io/kedacore/keda:2.21.0` | 0 | 0 |
| `grafana/alloy:v1.20.1` | 1 | 0 |
| `grafana/grafana:13.2.0` | 33 | 0 |
| `grafana/loki:3.7.8` | 0 | 0 |
| `grafana/tempo:2.10.3` | 45 | 1 |
| `otel/opentelemetry-collector:0.147.0` | 46 | 1 |
| `postgres:18.6-bookworm` | 49 | 7 |
| `prom/node-exporter:v1.12.1` | 9 | 0 |
| `prom/prometheus:v3.14.0` | 3 | 0 |
| `public.ecr.aws/docker/library/redis:8.2.3-alpine` | 14 | 1 |
| `quay.io/argoproj/argocd:v3.5.3` | 44 | 1 |
| `quay.io/calico/kube-controllers:v3.32.2` | 2 | 0 |
| `quay.io/calico/node:v3.32.2` | 11 | 0 |
| `quay.io/prometheuscommunity/postgres-exporter:v0.20.1` | 11 | 0 |
| `quay.io/strimzi/kafka:1.2.0-kafka-4.3.1` | 100 | 2 |
| `quay.io/strimzi/operator:1.2.0` | 31 | 0 |
| `rancher/local-path-provisioner:v0.0.32` | 36 | 3 |
| `registry.k8s.io/coredns/coredns:v1.14.6` | 26 | 0 |
| `registry.k8s.io/etcd:3.7.0-0` | 13 | 0 |
| `registry.k8s.io/kube-apiserver:v1.37.1` | 3 | 0 |
| `registry.k8s.io/kube-controller-manager:v1.37.1` | 3 | 0 |
| `registry.k8s.io/kube-proxy:v1.37.1` | 5 | 0 |
| `registry.k8s.io/kube-scheduler:v1.37.1` | 3 | 0 |
| `registry.k8s.io/kube-state-metrics/kube-state-metrics:v2.20.0` | 4 | 0 |

## Evidence and limits

The [test evidence](security-review-evidence.json) contains sanitised responses and fixture results. The [image inventory](security-image-inventory.json) records scanned running digests, counts and package findings. Scanner severities describe package advisories; they are not proof that all reported paths are reachable in this deployment.

Tools: Trivy 0.75.0, Gitleaks 8.30.1, OSV's package/version API and vendor advisory feeds. Scans ran locally; the cluster was used for small probes and disposable fixtures. Registry tags were checked again using the exact running digests. Temporary cache-lock failures were retried.

Cloudflare's managed implementation, host RPM vulnerability scanning, MFA effectiveness, a full SSRF/XSS review, secret rotation, backup restoration and distributed load attacks remain outside this review. TLS and database permissions were observed directly; package exploitability needs further triage. API/database traffic should also gain TLS or an explicitly justified network boundary.

Before promoting the portfolio, link this review and show the follow-up fixes. Backup restoration, rollback timing and measured capacity results from the [operations notes](operations.md) remain useful SRE evidence to add.
