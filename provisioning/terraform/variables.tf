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

variable "environment" {
  description = "environment name"
  type        = string
}

variable "application" {
  description = "Application name used in resource names and tags."
  type        = string
}

variable "image_ocid" {
  description = "x86_64-compatible OCI image OCID for the Kubernetes instances."
  type        = string
}

variable "instance_shape" {
  description = "Paid OCI flexible VM shape used by the Kubernetes instances."
  type        = string
  default     = "VM.Standard.E5.Flex"
}

variable "ansible_wsl_directory" {
  description = "Path to the Ansible directory used by Terraform's Ansible action."
  type        = string
  default     = "/mnt/c/Users/Gary/Documents/dummy_exchange/provisioning/ansible"
}

variable "ansible_runner" {
  description = "Operating system of the host running Terraform: windows or linux."
  type        = string
  default     = "windows"

  validation {
    condition     = contains(["windows", "linux"], var.ansible_runner)
    error_message = "ansible_runner must be windows or linux."
  }
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
