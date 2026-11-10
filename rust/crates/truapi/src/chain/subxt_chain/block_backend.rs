//! [`BlockBackend`] for `ChainRuntime`.

use sp_crypto_hashing::blake2_256;
use subxt::{
	SubstrateConfig,
	backend::Backend,
	config::Header,
	error::DispatchError,
	events::{Events, Phase},
	metadata::ArcMetadata,
	utils::H256,
};

use super::{failure, finalized_block};
use crate::{
	chain::{BlockBackend, DispatchOutcome, HashAndNumber},
	chain_runtime::{ChainRuntime, LegacyConnection, RuntimeFailure, encode_hex},
};

const DISPATCH_OUTCOME: &str = "dispatch_outcome";

#[async_trait::async_trait]
impl BlockBackend for ChainRuntime {
	async fn block_hash(&self, genesis: H256, number: u64) -> Result<Option<H256>, RuntimeFailure> {
		const METHOD: &str = "block_hash";
		let legacy = self.legacy(genesis).await?;
		if let Some(block) = legacy
			.backend
			.block_number_to_hash(number)
			.await
			.map_err(|error| failure(METHOD, error))?
		{
			return Ok(Some(block.hash()));
		}
		// Every height up to the finalized one has a block, so a missing hash
		// there is one the node cannot serve, not one that does not exist.
		if number <= finalized_block(&legacy, METHOD).await?.number {
			return Err(RuntimeFailure::host_failure(
				METHOD,
				format!("node cannot serve the hash of finalized block {number}"),
			));
		}
		Ok(None)
	}

	async fn block_number(&self, genesis: H256, hash: H256) -> Result<Option<u64>, RuntimeFailure> {
		const METHOD: &str = "block_number";
		let header = self
			.legacy(genesis)
			.await?
			.backend
			.block_header(hash)
			.await
			.map_err(|error| failure(METHOD, error))?;
		Ok(header.map(|header| header.number()))
	}

	async fn extrinsic_hashes(
		&self,
		genesis: H256,
		at: H256,
	) -> Result<Option<Vec<H256>>, RuntimeFailure> {
		const METHOD: &str = "extrinsic_hashes";
		let body = body(&self.legacy(genesis).await?, at, METHOD).await?;
		Ok(body.map(|extrinsics| {
			extrinsics.iter().map(|extrinsic| H256(blake2_256(extrinsic))).collect()
		}))
	}

	async fn dispatch_outcome(
		&self,
		genesis: H256,
		at: HashAndNumber,
		extrinsic_hash: H256,
	) -> Result<Option<DispatchOutcome>, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		let Some(phase) = extrinsic_phase(&legacy, at.hash, extrinsic_hash).await? else {
			return Ok(None);
		};
		let (events, metadata) = block_events(&legacy, at).await?;
		let outcome = dispatch_event(&events, &metadata, phase)?.ok_or_else(|| {
			RuntimeFailure::host_failure(
				DISPATCH_OUTCOME,
				format!("no dispatch event for {phase:?} of block {:?}", at.hash),
			)
		})?;
		Ok(Some(outcome))
	}
}

/// The phase in which the extrinsic with `extrinsic_hash` ran in block `at`,
/// or `None` when the block is unknown or does not contain it.
async fn extrinsic_phase(
	legacy: &LegacyConnection,
	at: H256,
	extrinsic_hash: H256,
) -> Result<Option<Phase>, RuntimeFailure> {
	let Some(body) = body(legacy, at, DISPATCH_OUTCOME).await? else {
		return Ok(None);
	};
	let Some(index) =
		body.iter().position(|extrinsic| H256(blake2_256(extrinsic)) == extrinsic_hash)
	else {
		return Ok(None);
	};
	let index = u32::try_from(index).map_err(|error| failure(DISPATCH_OUTCOME, error))?;
	Ok(Some(Phase::ApplyExtrinsic(index)))
}

/// `System.Events` of block `at`, and the metadata their fields decode
/// against: that of the runtime that produced the block.
async fn block_events(
	legacy: &LegacyConnection,
	at: HashAndNumber,
) -> Result<(Events<SubstrateConfig>, ArcMetadata), RuntimeFailure> {
	let block = legacy
		.client
		.at_block_hash_and_number(at.hash, at.number)
		.await
		.map_err(|error| failure(DISPATCH_OUTCOME, error))?;
	let events = block.events().fetch().await.map_err(|error| failure(DISPATCH_OUTCOME, error))?;
	Ok((events, block.metadata()))
}

/// The outcome the `System` dispatch event emitted in `phase` reports, or
/// `None` when there is no such event.
fn dispatch_event(
	events: &Events<SubstrateConfig>,
	metadata: &ArcMetadata,
	phase: Phase,
) -> Result<Option<DispatchOutcome>, RuntimeFailure> {
	for event in events.iter() {
		let event = event.map_err(|error| failure(DISPATCH_OUTCOME, error))?;
		if event.phase() != phase || event.pallet_name() != "System" {
			continue;
		}
		match event.event_name() {
			"ExtrinsicSuccess" => return Ok(Some(DispatchOutcome::Succeeded)),
			"ExtrinsicFailed" => {
				// The event alone proves the failure, so an error that does not
				// decode is reported as such instead of hiding the outcome.
				let fields = event.field_bytes();
				let error = DispatchError::decode_from(fields, metadata.clone()).map_or_else(
					|error| format!("undecodable dispatch error {}: {error}", encode_hex(fields)),
					|error| error.to_string(),
				);
				return Ok(Some(DispatchOutcome::Failed(error)));
			},
			_ => {},
		}
	}
	Ok(None)
}

/// The body of block `at`, or `None` when the node does not know the block.
/// A node that knows the header but not the body cannot prove what the block
/// contains, so that is an error.
async fn body(
	legacy: &LegacyConnection,
	at: H256,
	method: &'static str,
) -> Result<Option<Vec<Vec<u8>>>, RuntimeFailure> {
	if let Some(body) =
		legacy.backend.block_body(at).await.map_err(|error| failure(method, error))?
	{
		return Ok(Some(body));
	}
	let known = legacy
		.backend
		.block_header(at)
		.await
		.map_err(|error| failure(method, error))?
		.is_some();
	if known {
		return Err(RuntimeFailure::host_failure(
			method,
			format!("node cannot serve the body of block {at:?}"),
		));
	}
	Ok(None)
}
