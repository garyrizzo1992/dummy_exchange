# Public API, CI runner, and application vault identifiers.

output "api_url" {
  value = var.cloudflare_enabled ? "https://${var.api_hostname}/v1" : null
}

output "cloudflare_tunnel_id" {
  description = "Non-secret ID. The connector token is fetched transiently by Ansible."
  value       = try(cloudflare_zero_trust_tunnel_cloudflared.api[0].id, null)
}

output "ci_runner_nsg_id" {
  description = "Attach this NSG to the private CI runner VNIC."
  value       = oci_core_network_security_group.ci_runner.id
}

output "ci_runner_id" {
  value = try(oci_core_instance.ci_runner[0].id, null)
}

output "application_vault_id" {
  value = oci_kms_vault.application.id
}

output "application_secret_key_id" {
  value = oci_kms_key.application.id
}

output "service_urls" {
  description = "Public URLs for additional Terraform-managed tunnel routes."
  value       = var.cloudflare_enabled ? { for name, route in local.active_service_routes : name => "https://${route.hostname}" } : {}
}

output "pending_access_hostnames" {
  description = "Protected hostnames awaiting Cloudflare Access activation; these are not published."
  value       = var.cloudflare_enabled ? [for name, route in var.cloudflare_service_routes : route.hostname if !var.cloudflare_access_enabled && length(route.access_emails) > 0] : []
}
