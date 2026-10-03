# OCI authentication

variable "region" {
  description = "OCI region in which this environment is managed."
  type        = string
  default     = "eu-milan-1"
}

variable "tenancy_ocid" {
  description = "OCI tenancy OCID used by the provider."
  type        = string
  sensitive   = true
}

variable "oci_auth" {
  description = "OCI authentication: APIKey locally or InstancePrincipal on the OCI runner."
  type        = string
  default     = "APIKey"

  validation {
    condition     = contains(["APIKey", "InstancePrincipal"], var.oci_auth)
    error_message = "oci_auth must be APIKey or InstancePrincipal."
  }
}

variable "user_ocid" {
  description = "OCI user OCID used by the provider."
  type        = string
  sensitive   = true
  ephemeral   = true
  default     = null
}

variable "fingerprint" {
  description = "Fingerprint of the OCI API signing key."
  type        = string
  sensitive   = true
  ephemeral   = true
  default     = null
}

variable "private_key_path" {
  description = "Absolute path to the local OCI API signing key."
  type        = string
  sensitive   = true
  ephemeral   = true
  default     = null
}

# Environment identity

variable "application" {
  description = "Application name used in resource names and tags."
  type        = string
}

variable "environment" {
  description = "environment name"
  type        = string
}

# Compute

variable "image_ocid" {
  description = "OCI image OCID matching instance_shape architecture (x86_64 or aarch64)."
  type        = string
}

variable "instance_shape" {
  description = "OCI flexible VM shape. Keep existing deployments unchanged; see examples/always-free.tfvars.example for A1 sizing."
  type        = string
  default     = "VM.Standard.E5.Flex"
}

variable "control_plane_ocpus" {
  description = "OCPUs for the kubeadm control-plane VM."
  type        = number
  default     = 2
}

variable "control_plane_memory_in_gbs" {
  description = "Memory in GB for the kubeadm control-plane VM."
  type        = number
  default     = 8
}

variable "worker_ocpus" {
  description = "OCPUs for the Kubernetes worker VM."
  type        = number
  default     = 2
}

variable "worker_memory_in_gbs" {
  description = "Memory in GB for the Kubernetes worker VM."
  type        = number
  default     = 8
}

variable "enable_ci_runner" {
  description = "Create the dedicated CI VM. Disable for a two-node A1 deployment using the full free compute allowance; private-runner workflows then require another runner."
  type        = bool
  default     = true
}

# Operator access

variable "ssh_authorized_keys" {
  description = "Public SSH key content authorized for the instance's default user."
  type        = string
}

variable "bastion_client_cidrs" {
  description = "Stable operator CIDRs allowed through Bastion; CI uses private SSH. Empty retains the local caller-IP fallback."
  type        = list(string)
  default     = []

  validation {
    condition     = alltrue([for cidr in var.bastion_client_cidrs : can(cidrnetmask(cidr)) && cidr != "0.0.0.0/0"])
    error_message = "Use valid restricted IPv4 CIDRs for Bastion operator access."
  }
}

# Cloudflare public access

variable "cloudflare_enabled" {
  description = "Manage the free Cloudflare Tunnel, API DNS record and HTTPS redirect."
  type        = bool
  default     = false
}

variable "cloudflare_account_id" {
  description = "Cloudflare account containing the tunnel."
  type        = string
  default     = ""
  validation {
    condition     = !var.cloudflare_enabled || can(regex("^[a-f0-9]{32}$", var.cloudflare_account_id))
    error_message = "cloudflare_account_id must be a 32-character account ID when enabled."
  }
}

variable "cloudflare_zone_id" {
  description = "Cloudflare zone owning the API DNS record."
  type        = string
  default     = ""
  validation {
    condition     = !var.cloudflare_enabled || can(regex("^[a-f0-9]{32}$", var.cloudflare_zone_id))
    error_message = "cloudflare_zone_id must be a 32-character zone ID when enabled."
  }
}

variable "api_hostname" {
  description = "Public API hostname routed directly to the API ClusterIP Service."
  type        = string
  default     = "api.garyrizzo.dev"
  validation {
    condition     = can(regex("^[a-z0-9]([a-z0-9.-]*[a-z0-9])?\\.[a-z]{2,}$", var.api_hostname))
    error_message = "api_hostname must be a lowercase DNS hostname, without a scheme or path."
  }
}

variable "api_service_url" {
  description = "Private Kubernetes API Service URL, including its Service port."
  type        = string
  default     = "http://dummy-exchange-dev-dummy-exchange-api.dummy-exchange.svc.cluster.local:3000"
  validation {
    condition     = can(regex("^https?://[a-z0-9.-]+[.]svc[.]cluster[.]local:[0-9]+$", var.api_service_url))
    error_message = "api_service_url must target a private Kubernetes Service DNS name and port."
  }
}

variable "cloudflare_service_routes" {
  description = "Additional public hostname routes to private Kubernetes Services. Keys identify Terraform-managed DNS records. Protect administrative services before publishing them."
  type = map(object({
    hostname = string
    service  = string
    path     = optional(string)
  }))
  default = {}
  validation {
    condition = alltrue([for route in values(var.cloudflare_service_routes) :
      can(regex("^[a-z0-9]([a-z0-9.-]*[a-z0-9])?[.][a-z]{2,}$", route.hostname)) &&
      can(regex("^https?://[a-z0-9.-]+[.]svc[.]cluster[.]local:[0-9]+$", route.service))
    ])
    error_message = "Routes require lowercase public DNS hostnames and private Kubernetes Service URLs with ports."
  }
  validation {
    condition     = length(distinct(concat([var.api_hostname], [for route in values(var.cloudflare_service_routes) : route.hostname]))) == length(var.cloudflare_service_routes) + 1
    error_message = "Public hostnames must be unique, including the API hostname."
  }
}
