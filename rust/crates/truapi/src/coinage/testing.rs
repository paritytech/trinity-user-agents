//! Fixtures shared by the coinage layer's tests.

use core::time::Duration;
use std::{collections::HashMap, sync::Arc};

use futures::executor::block_on;
use parity_scale_codec::Decode;
use subxt::{Metadata, utils::H256};

use super::{
	ChainConfig, ChainParams,
	operation::OpKey,
	params::{InstanceRecord, validate},
	registration::{Admission, RegisterError, Rows, coinage_domain, register, registry_domain},
};
use crate::{
	durable::{
		DeferredDurableTx, DurableRegistry, DurableRequest, DurableTxEngine, DurableTxEntry, DurableTxId,
		PolicyError, PolicyParams, Preparation, RegistrationError, SubmissionPolicy, Unobservable,
		VerdictFailureKind,
	},
	store::Db,
};

/// Metadata of the Paseo Next People chain, which runs the coinage pallet.
pub const PEOPLE_METADATA_BYTES: &[u8] =
	include_bytes!("../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

/// [`PEOPLE_METADATA_BYTES`], decoded.
pub fn people_metadata() -> Metadata {
	let prefixed =
		frame_metadata::RuntimeMetadataPrefixed::decode(&mut &PEOPLE_METADATA_BYTES[..])
			.expect("the People fixture decodes");
	Metadata::try_from(prefixed).expect("the People fixture is subxt metadata")
}

/// A configuration with the shipped values for the Paseo Next People chain.
pub fn chain_config() -> ChainConfig {
	ChainConfig {
		instance: 0,
		extension_version: 0,
		ring_lifetime: Duration::from_secs(90 * 24 * 3600),
		ring_page_size: 255,
		token_grace: Duration::from_secs(3600),
		unload_allowance: 20,
		external_minimum_balance: 1,
		block_time: Duration::from_secs(6),
		rescue_margin: Duration::from_secs(7 * 24 * 3600),
		reserve_share: 20,
		fallback_age_cutoff: 14,
		longest_window: 64,
		max_claim_window: 256,
	}
}

/// The record of the instance [`chain_config`] names.
pub fn instance_record() -> InstanceRecord {
	InstanceRecord { asset_unit: 10_000 }
}

/// Parameters validated from the People fixture with [`chain_config`] and a
/// maximum coin age of 16.
pub fn chain_params() -> ChainParams {
	validate(&people_metadata(), Some(&instance_record()), Some(16), chain_config())
		.expect("the People fixture serves the layer")
}

/// A durability engine over `db` that serves both coinage domains and never
/// builds or decides anything, so records keep the status a test writes.
pub fn unbuilt_engine(db: Db) -> Arc<DurableTxEngine> {
	let registry = DurableRegistry::new()
		.with_domain(coinage_domain(), H256::zero(), Arc::new(Unobservable), Arc::new(NeverBuilds))
		.with_domain(registry_domain(), H256::zero(), Arc::new(Unobservable), Arc::new(NeverBuilds));
	crate::durable::engine_on(db, registry)
}

/// A policy that leaves every record waiting to be built.
struct NeverBuilds;

#[async_trait::async_trait]
impl SubmissionPolicy for NeverBuilds {
	fn can_retry(&self, _: &DurableTxEntry, _: &PolicyParams, _: VerdictFailureKind) -> bool {
		false
	}

	async fn prepare(&self, _: &[DeferredDurableTx]) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		Ok(HashMap::new())
	}
}

/// Registers one record per entry of `rows` under `operation`, writing C1's
/// rows in the registration callback, and returns their identities.
pub fn register_rows(
	engine: &Arc<DurableTxEngine>,
	operation: &OpKey,
	admission: Admission,
	rows: Vec<Rows>,
) -> Result<Vec<DurableTxId>, RegisterError> {
	let count = rows.len();
	let request = DurableRequest::deferred_in(coinage_domain(), Some(operation.group()), move |_| {
		Ok(vec![PolicyParams(Vec::new()); count])
	});
	let operation = operation.clone();
	let registered = block_on(engine.execute_with(request, move |tx, ids| {
		let records: Vec<_> = ids.iter().copied().zip(rows).collect();
		register(tx, &coinage_domain(), &operation, admission, &records).expect("the rows are readable")
	}));
	match registered {
		Ok((ids, ())) => Ok(ids),
		Err(RegistrationError::Hook(error)) => Err(error),
		Err(other) => panic!("the registration did not run: {other}"),
	}
}

/// Writes the durability layer's status of `record`; a success is recorded
/// at block 1.
pub fn set_status(db: &Db, record: DurableTxId, status: &'static str, failure: Option<&'static str>) {
	let success = status.ends_with("SUCCESS");
	block_on(db.write(move |tx| {
		tx.execute(
			"UPDATE durable_tx SET status = ?1, failure_kind = ?2, success_number = ?3, success_hash = ?4,
			 tx_hash = ?5 WHERE id = ?6",
			rusqlite::params![
				status,
				failure,
				success.then_some(1),
				success.then_some([1u8; 32].to_vec()),
				[record.0 as u8; 32].to_vec(),
				record.0,
			],
		)?;
		Ok(())
	}))
	.expect("the status is written");
}
