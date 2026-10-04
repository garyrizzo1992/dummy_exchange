environment = "dev"
application = "dummy-exchange"

# DNS and public routing use Cloudflare's Free-plan Tunnel, not a public OCI LB.
cloudflare_enabled    = true
cloudflare_account_id = "81ff770ff712c1f376f115fbacc80f5e"
cloudflare_zone_id    = "e056771565ac305a67753e7ee753ed5a"
api_hostname          = "api.garyrizzo.dev"

cloudflare_service_routes = {
  grafana = {
    hostname = "grafana.garyrizzo.dev"
    service  = "http://dummy-exchange-dev-dummy-exchange-grafana.dummy-exchange.svc.cluster.local:3000"
  }
  argocd = {
    hostname      = "argocd.garyrizzo.dev"
    service       = "https://argocd-server.argocd.svc.cluster.local:443"
    no_tls_verify = true # Argo CD uses a self-signed certificate inside the dev cluster.
  }
  prometheus = {
    hostname      = "prometheus.garyrizzo.dev"
    service       = "http://dummy-exchange-dev-dummy-exchange-prometheus.dummy-exchange.svc.cluster.local:9090"
    access_emails = ["1992rizzogary@gmail.com"]
  }
}

# Cloudflare Access protects Prometheus with email-code login.
cloudflare_access_enabled = true
