# Plans only; provider mocks prevent OCI/Cloudflare access and Ansible actions.
mock_provider "oci" {
  mock_data "oci_identity_availability_domains" {
    defaults = {
      availability_domains = [{ name = "test-ad" }]
    }
  }
}
mock_provider "http" {
  mock_data "http" {
    defaults = { response_body = "192.0.2.1" }
  }
}
mock_provider "ansible" {}
mock_provider "cloudflare" {
  mock_resource "cloudflare_zero_trust_tunnel_cloudflared" {
    override_during = plan
    defaults        = { id = "01234567-89ab-cdef-0123-456789abcdef" }
  }
}

variables {
  tenancy_ocid          = "ocid1.tenancy.oc1..test"
  environment           = "test"
  application           = "dummy-exchange"
  image_ocid            = "ocid1.image.oc1.test"
  ssh_authorized_keys   = "ssh-ed25519 test"
  cloudflare_account_id = "81ff770ff712c1f376f115fbacc80f5e"
  cloudflare_zone_id    = "e056771565ac305a67753e7ee753ed5a"
  api_hostname          = "api.garyrizzo.dev"
}

run "private_cluster_without_cloudflare" {
  command = plan
  assert {
    condition     = length(cloudflare_dns_record.api) == 0 && length(cloudflare_zero_trust_tunnel_cloudflared.api) == 0
    error_message = "Cloudflare is opt-in and must not expose an unconfigured cluster."
  }
}

run "free_tunnel_and_dns" {
  command = plan
  variables { cloudflare_enabled = true }

  assert {
    condition     = cloudflare_dns_record.api[0].type == "CNAME" && cloudflare_dns_record.api[0].proxied && cloudflare_dns_record.api[0].ttl == 1
    error_message = "The API must use proxied DNS to the tunnel, without an OCI public IP."
  }
  assert {
    condition     = cloudflare_dns_record.api[0].content == "01234567-89ab-cdef-0123-456789abcdef.cfargotunnel.com"
    error_message = "DNS must resolve to the Terraform-owned tunnel."
  }
  assert {
    condition     = cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress[0].path == "^/v1(/.*)?$" && cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress[1].service == "http_status:404"
    error_message = "Only API paths may be published; all other paths must return 404."
  }
  assert {
    condition     = cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress[0].origin_request.http_host_header == var.api_hostname
    error_message = "The tunnel must preserve the public API hostname."
  }
  assert {
    condition     = cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress[0].service == "http://dummy-exchange-dev-dummy-exchange-api.dummy-exchange.svc.cluster.local:3000"
    error_message = "The origin must remain a private cluster service."
  }
  assert {
    condition     = cloudflare_zone_setting.https[0].value == "on"
    error_message = "Visitors must be redirected to HTTPS."
  }
  assert {
    condition     = !oci_core_instance.control_plane.create_vnic_details[0].assign_public_ip && !oci_core_instance.worker.create_vnic_details[0].assign_public_ip
    error_message = "Tunnel access must not give the Kubernetes nodes public IPs."
  }
}

run "additional_services" {
  command = plan
  variables {
    cloudflare_enabled = true
    cloudflare_service_routes = {
      grafana = {
        hostname = "grafana.garyrizzo.dev"
        service  = "http://dummy-exchange-dev-dummy-exchange-grafana.dummy-exchange.svc.cluster.local:3000"
      }
      argocd = {
        hostname      = "argocd.garyrizzo.dev"
        service       = "https://argocd-server.argocd.svc.cluster.local:443"
        no_tls_verify = true
      }
    }
  }
  assert {
    condition = alltrue([for key, route in var.cloudflare_service_routes :
      cloudflare_dns_record.services[key].name == route.hostname &&
      cloudflare_dns_record.services[key].proxied &&
      cloudflare_dns_record.services[key].content == "01234567-89ab-cdef-0123-456789abcdef.cfargotunnel.com" &&
      contains([for rule in cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress : rule.service], route.service)
    ])
    error_message = "Each additional hostname needs proxied tunnel DNS and its own direct Service destination."
  }
  assert {
    condition     = cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress[1].origin_request.no_tls_verify == true && cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress[1].service == "https://argocd-server.argocd.svc.cluster.local:443"
    error_message = "Argo CD must use HTTPS with its development self-signed certificate accepted only on its own route."
  }
  assert {
    condition     = length(cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress) == 4 && cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress[3].service == "http_status:404"
    error_message = "Additional routes must retain the final 404 catch-all."
  }
}

run "protected_route_waits_for_access" {
  command = plan
  variables {
    cloudflare_enabled = true
    cloudflare_service_routes = {
      prometheus = {
        hostname      = "prometheus.garyrizzo.dev"
        service       = "http://dummy-exchange-dev-dummy-exchange-prometheus.dummy-exchange.svc.cluster.local:9090"
        access_emails = ["operator@example.com"]
      }
    }
  }
  assert {
    condition     = length(cloudflare_dns_record.services) == 0 && length(cloudflare_zero_trust_access_application.services) == 0 && length(cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress) == 2
    error_message = "Protected routes must remain absent from DNS and Tunnel until Access is enabled."
  }
  assert {
    condition     = length(output.pending_access_hostnames) == 1 && one(output.pending_access_hostnames) == "prometheus.garyrizzo.dev" && length(output.service_urls) == 0
    error_message = "Outputs must identify unpublished protected routes rather than advertise them as accessible."
  }
}

run "protected_route_with_email_login" {
  command = plan
  variables {
    cloudflare_enabled        = true
    cloudflare_access_enabled = true
    cloudflare_service_routes = {
      prometheus = {
        hostname      = "prometheus.garyrizzo.dev"
        service       = "http://dummy-exchange-dev-dummy-exchange-prometheus.dummy-exchange.svc.cluster.local:9090"
        access_emails = ["operator@example.com"]
      }
    }
  }
  assert {
    condition     = cloudflare_zero_trust_access_application.services["prometheus"].domain == cloudflare_dns_record.services["prometheus"].name && cloudflare_zero_trust_access_application.services["prometheus"].policies[0].decision == "allow" && one(cloudflare_zero_trust_access_application.services["prometheus"].policies[0].include).email.email == "operator@example.com"
    error_message = "Prometheus must be protected by an allow policy for the specified operator email."
  }
  assert {
    condition     = cloudflare_zero_trust_access_identity_provider.email[0].type == "onetimepin" && length(cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config.ingress) == 3
    error_message = "Protected routes must have email-code login and a tunnel destination."
  }
}
