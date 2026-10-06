# Free-plan public API tunnel, routing, DNS, and HTTPS redirect.

resource "cloudflare_zero_trust_tunnel_cloudflared" "api" {
  count      = var.cloudflare_enabled ? 1 : 0
  account_id = var.cloudflare_account_id
  name       = "${var.application}-${var.environment}-api"
  config_src = "cloudflare"
}

# Nodes depend on this barrier so destroy stops the connectors first, then
# waits for Cloudflare to expire their connections before deleting the tunnel.
resource "terraform_data" "cloudflare_tunnel_drain" {
  count = var.cloudflare_enabled ? 1 : 0
  input = {
    wait_seconds = var.cloudflare_tunnel_destroy_wait_seconds
  }

  depends_on = [cloudflare_zero_trust_tunnel_cloudflared.api]

  provisioner "local-exec" {
    when    = destroy
    command = "echo Waiting ${self.input.wait_seconds}s for Cloudflare Tunnel connections to close; sleep ${self.input.wait_seconds}"
  }
}

resource "cloudflare_zero_trust_tunnel_cloudflared_config" "api" {
  count      = var.cloudflare_enabled ? 1 : 0
  account_id = var.cloudflare_account_id
  tunnel_id  = cloudflare_zero_trust_tunnel_cloudflared.api[0].id
  depends_on = [cloudflare_zero_trust_access_application.services]
  config = {
    ingress = concat([
      {
        hostname = var.api_hostname
        path     = "^/v1(/.*)?$"
        # The external connection is encrypted by Cloudflare Tunnel. Only the
        # final hop inside this private development cluster uses HTTP.
        service = var.api_service_url
        origin_request = {
          http_host_header = var.api_hostname
        }
      }
      ], [for route in values(local.active_service_routes) : merge(
        { hostname = route.hostname, service = route.service },
        route.path == null ? {} : { path = route.path },
        route.no_tls_verify ? { origin_request = { no_tls_verify = true } } : {}
    )], [{ service = "http_status:404" }])
  }
}

resource "cloudflare_dns_record" "api" {
  count      = var.cloudflare_enabled ? 1 : 0
  zone_id    = var.cloudflare_zone_id
  name       = var.api_hostname
  type       = "CNAME"
  content    = "${cloudflare_zero_trust_tunnel_cloudflared.api[0].id}.cfargotunnel.com"
  proxied    = true
  ttl        = 1
  comment    = "Terraform: private Kubernetes API via free Cloudflare Tunnel"
  depends_on = [cloudflare_zero_trust_tunnel_cloudflared_config.api]
}

resource "cloudflare_zone_setting" "https" {
  count      = var.cloudflare_enabled ? 1 : 0
  zone_id    = var.cloudflare_zone_id
  setting_id = "always_use_https"
  value      = "on"
}

resource "cloudflare_dns_record" "services" {
  for_each   = var.cloudflare_enabled ? local.active_service_routes : {}
  zone_id    = var.cloudflare_zone_id
  name       = each.value.hostname
  type       = "CNAME"
  content    = "${cloudflare_zero_trust_tunnel_cloudflared.api[0].id}.cfargotunnel.com"
  proxied    = true
  ttl        = 1
  comment    = "Terraform: private Kubernetes service via free Cloudflare Tunnel"
  depends_on = [cloudflare_zero_trust_tunnel_cloudflared_config.api]
}

# Create Access before adding protected hostnames to Tunnel or DNS.
resource "cloudflare_zero_trust_access_identity_provider" "email" {
  count      = var.cloudflare_enabled && length(local.protected_service_routes) > 0 ? 1 : 0
  account_id = var.cloudflare_account_id
  name       = "${var.application}-${var.environment}-email"
  type       = "onetimepin"
  config     = {}
}

resource "cloudflare_zero_trust_access_application" "services" {
  for_each                  = var.cloudflare_enabled ? local.protected_service_routes : {}
  account_id                = var.cloudflare_account_id
  name                      = "${var.application}-${var.environment}-${each.key}"
  domain                    = each.value.hostname
  type                      = "self_hosted"
  session_duration          = "24h"
  allowed_idps              = [cloudflare_zero_trust_access_identity_provider.email[0].id]
  auto_redirect_to_identity = true
  policies = [{
    name       = "Development operators"
    decision   = "allow"
    precedence = 1
    include    = [for email in sort(tolist(each.value.access_emails)) : { email = { email = email } }]
  }]
}
