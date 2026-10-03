terraform {
  # Provider actions require Terraform 1.14 or newer.
  required_version = ">= 1.14.0"

  required_providers {
    ansible = {
      source  = "ansible/ansible"
      version = "~> 1.4"
    }

    oci = {
      source  = "oracle/oci"
      version = "9.3.0"
    }
    http = {
      source  = "hashicorp/http"
      version = "3.6.2"
    }
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "5.24.0"
    }
  }
}
