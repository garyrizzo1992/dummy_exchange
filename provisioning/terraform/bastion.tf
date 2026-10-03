# Managed operator access; an empty CIDR list falls back to the Terraform caller IP.

data "http" "terraform_runner_ip" {
  url = "https://api.ipify.org"
}

resource "oci_bastion_bastion" "bastion" {
  #Required
  bastion_type               = "STANDARD"
  compartment_id             = oci_identity_compartment.dummy_exchange.id
  target_subnet_id           = oci_core_subnet.dummy_exchange_priv.id
  max_session_ttl_in_seconds = 10800

  #Optional
  client_cidr_block_allow_list = length(var.bastion_client_cidrs) > 0 ? var.bastion_client_cidrs : ["${trimspace(data.http.terraform_runner_ip.response_body)}/32"]
  freeform_tags                = local.freeform_tags
  name                         = "${var.application}-${var.environment}-bastion"
}
