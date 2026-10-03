environment = "dev"
application = "dummy-exchange"

# DNS and public routing use Cloudflare's Free-plan Tunnel, not a public OCI LB.
cloudflare_enabled    = true
cloudflare_account_id = "81ff770ff712c1f376f115fbacc80f5e"
cloudflare_zone_id    = "e056771565ac305a67753e7ee753ed5a"
api_hostname          = "api.garyrizzo.dev"
