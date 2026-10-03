# Free Cloudflare access

This environment uses Free-plan DNS, HTTPS redirect and Tunnel. It provisions
no paid Cloudflare features or public OCI load balancer. Existing OCI compute,
NAT and other infrastructure retain their existing billing.

```text
https://api.garyrizzo.dev/v1/...
  -> Cloudflare edge TLS -> encrypted outbound Tunnel
  -> private API ClusterIP Service -> API pods
```

[Tunnel is available on all plans](https://developers.cloudflare.com/tunnel/)
and connects outbound without a public origin IP. Two connector replicas share
one tunnel; on the single worker they provide process redundancy, not node
availability. The final hop to the API Service is HTTP inside the private development
cluster. Browser traffic and Tunnel transport are encrypted. For untrusted
internal networks, provision internal TLS and change the tunnel origin accordingly. Only `/v1` is published; other paths return `404`.

## Ownership

[`cloudflare.tf`](../cloudflare.tf) owns the tunnel, hostname routing, proxied CNAME and zone-wide
free `Always Use HTTPS` setting. It does not change the zone plan, registrar,
nameservers, root-domain records or other DNS records. Public identifiers are
in [`envs/dev.tfvars`](../envs/dev.tfvars); `api_service_url` identifies the API Service and port.

Argo owns cloudflared and the application Services. Terraform's Ansible action fetches the
connector token directly from Cloudflare and passes it through stdin to the
Kubernetes Secret, with logging disabled. The API token stays on the controller.
Neither token is a Terraform input/output; the tunnel token data source is
deliberately absent so it is not persisted in Terraform state. Kubernetes stores
the connector Secret, so protect Secret access and etcd storage.

## Credentials

`provisioning/.cloudflare` contains one raw **API token**, not a Global API Key,
and remains ignored by Git. The supplied token can read DNS but returned `403`
for tunnel access. Update its permissions, scoped to this account and zone:

- Account: **Cloudflare Tunnel — Edit**.
- Zone: **DNS — Edit** for `garyrizzo.dev`.
- Zone: **Zone Settings — Edit** for HTTPS redirect.
- Zone: **Zone — Read** for zone inspection.

These permissions need no paid plan. Add the same API token to the GitHub `dev`
environment as secret `CLOUDFLARE_API_TOKEN` for infrastructure apply/drift checks.
Do not put it in committed `.tfvars`, plans or generated manifests.

## Plan and deploy

From the repository root in WSL or Linux:

```bash
bash scripts/terraform-cloudflare.sh init
bash scripts/terraform-cloudflare.sh plan -var-file=envs/dev.tfvars -out=cloudflare.tfplan
# Review the whole plan, including any pre-existing infrastructure changes.
bash scripts/terraform-cloudflare.sh apply cloudflare.tfplan
```

The wrapper loads the ignored token into `CLOUDFLARE_API_TOKEN` and restores the
previous environment afterward. On Linux/CI, set that environment variable
through the secret manager and run Terraform normally.

Complete the staged five-chart ownership transfer before reprovisioning a legacy
business application. Ansible checks that first, then configures the connector Secret and the tunnel application. DNS alone cannot serve the API
until the connectors and business application are healthy.

The API hostname had no DNS record during setup. If another operator creates
one before apply, import it instead of creating a duplicate:

```bash
bash scripts/terraform-cloudflare.sh import -var-file=envs/dev.tfvars 'cloudflare_dns_record.api[0]' '<zone-id>/<dns-record-id>'
```

After connector token rotation, rerun the Ansible action through a reviewed plan
with `-replace=terraform_data.ansible`. Changed Secrets restart existing connector
pods. Before intentionally removing the tunnel, disable the connector Argo app;
`cloudflare_enabled=false` removes Cloudflare resources, not the Argo application.

## Verify

```bash
terraform -chdir=provisioning/terraform validate
terraform -chdir=provisioning/terraform test
python scripts/check-helm.py
curl https://api.garyrizzo.dev/v1/instruments
curl -I https://api.garyrizzo.dev/metrics
```

The metrics request must return `404`. Terraform tests use mocked providers and
plan-only runs, creating no resources and running no Ansible actions. Kubernetes
manifest checks include private application Services and the two-replica connector.

## Additional service hostnames

No DNS records existed in this zone when checked on 2026-10-04. The current
API token still returned HTTP 403 for tunnel access, so these changes have not
been applied. Only the API hostname is configured by default.

Additional routes can be added to `envs/dev.tfvars`, for example:

```hcl
cloudflare_service_routes = {
  grafana = {
    hostname = "grafana.garyrizzo.dev"
    service  = "http://dummy-exchange-dev-dummy-exchange-grafana.dummy-exchange.svc.cluster.local:3000"
  }
  prometheus = {
    hostname = "prometheus.garyrizzo.dev"
    service  = "http://dummy-exchange-dev-dummy-exchange-prometheus.dummy-exchange.svc.cluster.local:9090"
  }
}
```

Terraform creates a proxied CNAME for each configured hostname and maps it to
its private Service through the same tunnel. Protect monitoring endpoints before
publishing them; Prometheus has no authentication in the current configuration.
The exchange simulator exposes only metrics and health, not a trading UI, and
remains private. Existing DNS records must be imported into the matching
`cloudflare_dns_record.services["grafana"]` address before managing them.

Bootstrap retires the old Traefik Argo application and labeled Traefik resources
after the connector becomes healthy. The existing `ingress` namespace remains
because it now hosts cloudflared, not an ingress controller.
