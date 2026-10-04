# Private VCN and subnet. NAT provides outbound Internet access for nodes and Tunnel.

resource "oci_core_vcn" "vcn" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  cidr_blocks    = ["10.0.0.0/16"]
  dns_label      = "dummyexchange"
  display_name   = "${var.application}-${var.environment}-vcn"
  freeform_tags  = local.freeform_tags
}

resource "oci_core_route_table" "private_instances" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id
  display_name   = "${var.application}-${var.environment}-private-vm-rt"

  route_rules {
    destination       = "0.0.0.0/0"
    destination_type  = "CIDR_BLOCK"
    network_entity_id = oci_core_nat_gateway.nat_gateway.id
  }

  freeform_tags = local.freeform_tags
}

resource "oci_core_subnet" "dummy_exchange_priv" {
  compartment_id            = oci_identity_compartment.dummy_exchange.id
  vcn_id                    = oci_core_vcn.vcn.id
  cidr_block                = "10.0.2.0/24"
  dns_label                 = "private"
  display_name              = "${var.application}-${var.environment}-private-vm-subnet"
  freeform_tags             = local.freeform_tags
  prohibit_internet_ingress = true
  route_table_id            = oci_core_route_table.private_instances.id
}

resource "oci_core_nat_gateway" "nat_gateway" {
  #Required
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id

  block_traffic = false
  display_name  = "${var.application}-${var.environment}-nat"
  freeform_tags = local.freeform_tags
}
