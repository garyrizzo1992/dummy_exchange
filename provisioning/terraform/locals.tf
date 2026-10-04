# Shared paths and resource tags.

locals {
  ansible_directory = abspath("${path.module}/../ansible")

  freeform_tags = {
    Application = var.application
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

locals {
  # Fail closed: unauthenticated services never receive a route before Access is ready.
  active_service_routes = {
    for name, route in var.cloudflare_service_routes : name => route
    if var.cloudflare_access_enabled || length(route.access_emails) == 0
  }
  protected_service_routes = {
    for name, route in local.active_service_routes : name => route
    if length(route.access_emails) > 0
  }
}
