use std::collections::BTreeSet;

use super::*;
use crate::{
	Spawner,
	generated::wire_table::WIRE_TABLE,
	host_core::HostRuntime,
	runtime::PairingHost,
	test_support::{runtime_config, stub_platform},
};

const NOT_PRODUCT_STARTED: [&str; 7] = [
	"funding_provider_quote",
	"funding_provider_assign",
	"funding_provider_cancel",
	"funding_provider_settle_cancel",
	"permissions_authorize_remote_permission",
	"permissions_authorize_device_permission",
	"renderer_render",
];

#[test]
fn a_worker_can_import_every_method_a_product_starts_under_its_wire_name() {
	let (config, product) = runtime_config("worker.dot");
	let host = HostRuntime::<PairingHost>::new(stub_platform(), config, Spawner::for_test());
	let admin = host.product_admin(product);
	let linked: BTreeSet<_> =
		WasmEnv::for_product(admin.product_runtime().clone()).method_names().collect();

	let product_started: BTreeSet<_> = WIRE_TABLE
		.iter()
		.map(|entry| entry.method)
		.filter(|method| !NOT_PRODUCT_STARTED.contains(method))
		.collect();
	assert_eq!(linked, product_started);
}
