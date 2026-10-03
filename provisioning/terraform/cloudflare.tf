# Free-plan public API tunnel, routing, DNS, and HTTPS redirect.

resource "cloudflare_zero_trust_tunnel_cloudflared" "api" {
  count      = var.cloudflare_enabled ? 1 : 0
  account_id = var.cloudflare_account_id
  name       = "${var.application}-${var.environment}-api"
  config_src = "cloudflare"
}

resource "cloudflare_zero_trust_tunnel_cloudflared_config" "api" {
  count      = var.cloudflare_enabled ? 1 : 0
  account_id = var.cloudflare_account_id
  tunnel_id  = cloudflare_zero_trust_tunnel_cloudflared.api[0].id
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
      ], [for route in values(var.cloudflare_service_routes) : merge(
        { hostname = route.hostname, service = route.service },
        route.path == null ? {} : { path = route.path }
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
  for_each   = var.cloudflare_enabled ? var.cloudflare_service_routes : {}
  zone_id    = var.cloudflare_zone_id
  name       = each.value.hostname
  type       = "CNAME"
  content    = "${cloudflare_zero_trust_tunnel_cloudflared.api[0].id}.cfargotunnel.com"
  proxied    = true
  ttl        = 1
  comment    = "Terraform: private Kubernetes service via free Cloudflare Tunnel"
  depends_on = [cloudflare_zero_trust_tunnel_cloudflared_config.api]
}
