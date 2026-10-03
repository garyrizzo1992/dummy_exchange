# Historical state address migrations; retain these for existing deployments.

moved {
  from = oci_core_instance.ci_runner
  to   = oci_core_instance.ci_runner[0]
}

moved {
  from = oci_identity_dynamic_group.ci_runner
  to   = oci_identity_dynamic_group.ci_runner[0]
}

moved {
  from = oci_identity_policy.ci_runner
  to   = oci_identity_policy.ci_runner[0]
}
