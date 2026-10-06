# CI uploads prebuilt static assets; Terraform owns the project and domain.
resource "cloudflare_pages_project" "frontend" {
  count             = var.cloudflare_pages_enabled ? 1 : 0
  account_id        = var.cloudflare_account_id
  name              = "${var.application}-${var.environment}-frontend"
  production_branch = "main"
}

resource "cloudflare_dns_record" "frontend" {
  count   = var.cloudflare_pages_enabled ? 1 : 0
  zone_id = var.cloudflare_zone_id
  name    = var.frontend_hostname
  type    = "CNAME"
  content = cloudflare_pages_project.frontend[0].subdomain
  proxied = true
  ttl     = 1
  comment = "Terraform: exchange static frontend on Cloudflare Pages"
}

resource "cloudflare_pages_domain" "frontend" {
  count        = var.cloudflare_pages_enabled ? 1 : 0
  account_id   = var.cloudflare_account_id
  project_name = cloudflare_pages_project.frontend[0].name
  name         = var.frontend_hostname
  depends_on   = [cloudflare_dns_record.frontend]
}
