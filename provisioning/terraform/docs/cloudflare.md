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
and remains ignored by Git. Scope its permissions to this account and zone:

- Account: **Cloudflare Tunnel — Edit**.
- Zone: **DNS — Edit** for `garyrizzo.dev`.
- Zone: **Zone Settings — Edit** for HTTPS redirect.
- Zone: **Zone — Read** for zone inspection.
- Account: **Access: Apps and Policies — Edit** and **Access: Organizations, Identity Providers, and Groups — Edit** for protected operator routes.

These permissions need no paid plan. Add the same API token to the GitHub `dev`
environment as secret `CLOUDFLARE_API_TOKEN` for infrastructure apply/drift checks.
Do not put it in committed `.tfvars`, plans or generated manifests.

## Plan and deploy

From `provisioning/terraform` in WSL or Linux:

```bash
terraform init
terraform plan -var-file=envs/dev.tfvars -out=cloudflare.tfplan
# Review the whole plan, including any pre-existing infrastructure changes.
terraform apply cloudflare.tfplan
```

The provider automatically reads `provisioning/.cloudflare` when that file exists,
trims trailing newlines and marks the value sensitive. The file takes precedence
locally. When it is absent, the provider uses `CLOUDFLARE_API_TOKEN`, as configured
in CI. Normal `terraform plan`, `apply` and `destroy` commands need no wrapper.

Complete the staged five-chart ownership transfer before reprovisioning a legacy
business application. Ansible checks that first, then configures the connector Secret and the tunnel application. DNS alone cannot serve the API
until the connectors and business application are healthy.

The API hostname had no DNS record during setup. If another operator creates
one before apply, import it instead of creating a duplicate:

```bash
terraform import -var-file=envs/dev.tfvars 'cloudflare_dns_record.api[0]' '<zone-id>/<dns-record-id>'
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

The dev environment configures Grafana and Argo CD alongside the API. Grafana
uses its existing application login; Argo CD uses its existing login and an HTTPS
origin with a self-signed development certificate. Terraform creates proxied
CNAME records and routes them to their private Kubernetes Services.

Prometheus is published with a Cloudflare Access email allow policy in dev, where
`cloudflare_access_enabled = true`. New accounts must activate Access and grant
the API token the Access edit permissions above before enabling this flag. Terraform creates the Access application
before its tunnel route or DNS record. With Access disabled, protected routes
are omitted and `pending_access_hostnames` reports them.

See [the complete dev service access inventory](dev-access.md) for hostnames,
management APIs, login requirements and private localhost-forwarding commands.
The simulator and workers expose metrics and health endpoints, not a trading UI.
Existing DNS records must be imported into the matching
`cloudflare_dns_record.services["grafana"]` (or other service) address before
Terraform can manage them.

Bootstrap retires the old Traefik Argo application and labeled Traefik resources
after the connector becomes healthy. The existing `ingress` namespace remains
because it now hosts cloudflared, not an ingress controller.

## Teardown

Full `terraform destroy` stops both Kubernetes nodes, then a built-in
`terraform_data` destroy provisioner waits 120 seconds before tunnel deletion.
Set `cloudflare_tunnel_destroy_wait_seconds` to adjust the delay. The local
provisioner requires a shell with `sleep` (WSL/Linux). This is a grace period,
not a check of Cloudflare connection status; if error 1022 persists, wait and
retry the destroy. Replicas outside these nodes must be stopped separately.

The barrier must already exist in state from an apply to run during destroy.
For an already partially destroyed environment, retry the remaining destroy
instead of applying the full configuration, which would recreate infrastructure.
