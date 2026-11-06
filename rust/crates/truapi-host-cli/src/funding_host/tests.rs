use super::*;

// The worker is told a top-up started only once the ledger holds it, as a
// start this host forgets would read as unknown after a restart and never
// settle.
#[test]
fn a_start_the_ledger_cannot_keep_is_refused() {
	let directory = tempfile::tempdir().expect("temporary directory");
	let host = CliPaymentHost { ledger: directory.path().join("missing").join("ledger.jsonl") };
	let product = ProductContext::new_with_execution(
		"ramp.dot".parse().expect("valid product"),
		truapi::platform::ExecutableKind::Worker,
	);

	let started = futures::executor::block_on(host.top_up(
		&product,
		HostPaymentTopUpRequest {
			into: None,
			amount: 1,
			source: truapi::latest::PaymentTopUpSource::ProductAccount {
				derivation_index: truapi::latest::DerivationIndex::Index(0),
			},
			id: [1; 32],
			funding: None,
		},
	));

	assert!(matches!(started, Err(HostPaymentTopUpError::Unknown { .. })), "{started:?}");
}
