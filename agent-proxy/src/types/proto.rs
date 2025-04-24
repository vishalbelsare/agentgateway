#[allow(warnings)]
#[warn(clippy::derive_partial_eq_without_eq)]
pub mod workload {
	tonic::include_proto!("istio.workload");
}
#[allow(warnings)]
#[warn(clippy::derive_partial_eq_without_eq)]
pub mod adp {
	tonic::include_proto!("istio.adp");
}
