# Environment foundation: project compartment and regional availability domains.

data "oci_identity_availability_domains" "available" {
  compartment_id = var.tenancy_ocid
}

resource "oci_identity_compartment" "dummy_exchange" {
  compartment_id = var.tenancy_ocid
  description    = "all resources relevant to dummy_exchange project"
  name           = "dummy-exchange"
  freeform_tags  = local.freeform_tags
}
