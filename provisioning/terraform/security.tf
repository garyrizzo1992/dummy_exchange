# Network security groups and traffic rules for Kubernetes nodes and the CI runner.

resource "oci_core_network_security_group" "kubernetes_nodes" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id
  display_name   = "${var.application}-${var.environment}-k8s-nodes"
}

resource "oci_core_network_security_group_security_rule" "kubernetes_node_to_node" {
  network_security_group_id = oci_core_network_security_group.kubernetes_nodes.id
  direction                 = "INGRESS"
  protocol                  = "all"
  source                    = oci_core_network_security_group.kubernetes_nodes.id
  source_type               = "NETWORK_SECURITY_GROUP"
  description               = "Allow Kubernetes node-to-node traffic"
}

resource "oci_core_network_security_group" "ci_runner" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id
  display_name   = "${var.application}-${var.environment}-ci-runner"
  freeform_tags  = local.freeform_tags
}

resource "oci_core_network_security_group_security_rule" "ci_runner_ssh" {
  network_security_group_id = oci_core_network_security_group.kubernetes_nodes.id
  direction                 = "INGRESS"
  protocol                  = "6"
  source                    = oci_core_network_security_group.ci_runner.id
  source_type               = "NETWORK_SECURITY_GROUP"
  description               = "Allow the dedicated CI runner to provision nodes over private SSH"

  tcp_options {
    destination_port_range {
      min = 22
      max = 22
    }
  }
}
