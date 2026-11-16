//! Funding accounts and policy parameters for the top-up and claim tests.

use std::sync::Arc;

use schnorrkel::{ExpansionMode, MiniSecretKey};

use crate::{
	coinage::{
		assembly::{Funding, KeypairFunding},
		operation::OpKey,
		registration::{Admission, RegisterError, Rows, coinage_domain, register},
		start::RecordParams,
	},
	durable::{DurableRequest, DurableTxEngine, DurableTxId, RegistrationError},
};

/// A funding account held in memory, a different one for each `seed`.
pub fn funding(seed: u8) -> Arc<dyn Funding> {
	let keypair = MiniSecretKey::from_bytes(&[seed; 32]).expect("32 bytes").expand_to_keypair(ExpansionMode::Ed25519);
	Arc::new(KeypairFunding(keypair))
}

/// Registers one record per entry of `records` under `operation`, each with
/// its rows and the policy parameters E3 would write for it, and returns
/// their identities in order.
pub fn register_records(
	engine: &Arc<DurableTxEngine>,
	operation: &OpKey,
	admission: Admission,
	records: Vec<(Rows, RecordParams)>,
) -> Result<Vec<DurableTxId>, RegisterError> {
	let params = records.iter().map(|(_, params)| params.to_params()).collect();
	let rows: Vec<Rows> = records.into_iter().map(|(rows, _)| rows).collect();
	let request = DurableRequest::deferred_in(coinage_domain(), Some(operation.group()), move |_| Ok(params));
	let operation = operation.clone();
	let registered = futures::executor::block_on(engine.execute_with(request, move |tx, ids| {
		let records: Vec<_> = ids.iter().copied().zip(rows).collect();
		register(tx, &coinage_domain(), &operation, admission, &records).expect("the rows are readable")
	}));
	match registered {
		Ok((ids, ())) => Ok(ids),
		Err(RegistrationError::Hook(error)) => Err(error),
		Err(other) => panic!("the registration did not run: {other}"),
	}
}
