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

variable "user_ocid" {
  description = "OCI user OCID used by the provider."
  type        = string
  sensitive   = true
  ephemeral   = true
}

variable "fingerprint" {
  description = "Fingerprint of the OCI API signing key."
  type        = string
  sensitive   = true
  ephemeral   = true
}

variable "private_key_path" {
  description = "Absolute path to the local OCI API signing key."
  type        = string
  sensitive   = true
  ephemeral   = true
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
  description = "ARM64-compatible OCI image OCID for the control-plane instance."
  type        = string
}

variable "ssh_authorized_keys" {
  description = "Public SSH key content authorized for the instance's default user."
  type        = string
}
