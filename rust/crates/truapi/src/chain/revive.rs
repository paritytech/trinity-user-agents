//! Contract reads and prepared writes over pallet-revive.

use alloy_sol_types::{GenericContractError, SolInterface};
use futures::StreamExt;
use parity_scale_codec::{Decode, Encode, Input};
use subxt::{ext::scale_encode::EncodeAsType, utils::H256};
use thiserror::Error;

use super::{
	BlockBackend, DispatchOutcome, EncodedExtrinsic, ExtrinsicBuildError, ExtrinsicService,
	HashAndNumber, RuntimeApiCaller, SubstrateSigner, TxSubmitter, WatchEvent,
};
use crate::chain_runtime::{ChainRuntime, RuntimeFailure};

const REVIVE_PALLET: &str = "Revive";

// Eth-derived origins are implicitly mapped; a nonzero address also passes
// contracts that reject address(0).
const VIEW_CALL_ORIGIN: [u8; 32] = [0xEE; 32];

/// Why a call through [`EvmContractCaller`] failed.
#[derive(Debug, Error)]
pub enum EvmContractError {
	/// The chain could not be reached or failed a request.
	#[error(transparent)]
	Chain(#[from] RuntimeFailure),
	/// The `ReviveApi_call` request failed.
	#[error("ReviveApi_call: {0}")]
	Simulation(String),
	/// The `ReviveApi_call` output is not a `ContractResult`. The cause is
	/// kept out of the message, which SCALE renders across several lines.
	#[error("ReviveApi_call output is not a ContractResult")]
	Malformed(parity_scale_codec::Error),
	/// The runtime rejected the call before reaching the contract.
	#[error("ReviveApi_call dispatch failed: {detail}")]
	Dispatch {
		/// Raw hex of the `DispatchError` payload.
		detail: String,
	},
	/// The contract executed but reverted.
	#[error("contract reverted: {detail}")]
	Reverted {
		/// Rendered revert reason.
		detail: String,
	},
	/// The signed extrinsic could not be built.
	#[error(transparent)]
	Build(#[from] ExtrinsicBuildError),
	/// The extrinsic ended without being finalized.
	#[error("the extrinsic was not finalized: {0:?}")]
	NotFinalized(WatchEvent),
	/// A finalized block included the extrinsic, which failed to dispatch.
	#[error("the extrinsic failed to dispatch")]
	DispatchFailed,
	/// The finalized block reports no dispatch outcome for the extrinsic.
	#[error("the finalized block reports no outcome for the extrinsic")]
	OutcomeMissing,
}

/// A simulated contract write bound to its signer and to the caller that
/// simulated it.
///
/// The request owns the calldata and limits. Its nonce, mortal era and
/// signature are selected when it is executed.
#[derive(derive_more::Debug)]
pub struct PreparedEvmTransaction<'a> {
	#[debug(skip)]
	caller: &'a EvmContractCaller,
	#[debug(skip)]
	signer: &'a SubstrateSigner,
	call: ReviveCallArgs,
}

/// Reads contracts and prepares writes on one chain.
pub struct EvmContractCaller {
	chain: ChainRuntime,
	extrinsics: ExtrinsicService,
	genesis: H256,
}

impl EvmContractCaller {
	/// Uses the configured chain and extrinsic builder for prepared writes.
	pub fn new(chain: ChainRuntime, extrinsics: ExtrinsicService, genesis: H256) -> Self {
		Self { chain, extrinsics, genesis }
	}

	/// Returns ABI data over the supplied runtime's read context.
	///
	/// Pinned readers retain their block and follow; plain RPC readers use
	/// the node's current state. No signing dependencies are needed.
	pub async fn read(
		runtime: &mut impl RuntimeApiCaller,
		contract: &[u8; 20],
		input: &[u8],
	) -> Result<Vec<u8>, EvmContractError> {
		Ok(simulate(runtime, &VIEW_CALL_ORIGIN, contract, input).await?.output)
	}

	/// Simulates as the signer and retains the exact call and required limits.
	///
	/// Nothing is signed or submitted. Chain state can change before execution.
	pub async fn dry_run<'a>(
		&'a self,
		signer: &'a SubstrateSigner,
		contract: &[u8; 20],
		input: &[u8],
	) -> Result<PreparedEvmTransaction<'a>, EvmContractError> {
		let account = signer.account_id();
		let connection = self.chain.legacy_connection(&self.genesis.0).await?;
		let mut rpc = connection.raw_rpc_client;
		let simulation = simulate(&mut rpc, &account.0, contract, input).await?;
		Ok(PreparedEvmTransaction {
			caller: self,
			signer,
			call: ReviveCallArgs {
				dest: *contract,
				weight_limit: simulation.weight_required,
				storage_deposit_limit: simulation.storage_deposit,
				data: input.to_vec(),
				..Default::default()
			},
		})
	}
}

impl PreparedEvmTransaction<'_> {
	/// Consumes the prepared call and waits for successful finalized dispatch.
	///
	/// The extrinsic gets a fresh nonce and mortal era and the prepared call's
	/// limits unchanged. The signer must keep the account it had at
	/// [`EvmContractCaller::dry_run`].
	pub async fn execute_and_watch(self) -> Result<(), EvmContractError> {
		let Self { caller, signer, call } = self;
		let extrinsic = caller
			.extrinsics
			.build_mortal_extrinsic(caller.genesis)
			.with_call_args(REVIVE_PALLET, "call", call)
			.with_signer(signer)
			.build()
			.await?;
		submit_until_finalized(&caller.chain, caller.genesis, &extrinsic.extrinsic).await
	}
}

async fn simulate(
	runtime: &mut impl RuntimeApiCaller,
	origin: &[u8; 32],
	contract: &[u8; 20],
	input: &[u8],
) -> Result<Simulation, EvmContractError> {
	let args = ReviveApiCallArgs {
		origin: *origin,
		dest: *contract,
		input_data: input,
		..Default::default()
	};
	let output = runtime
		.call("ReviveApi_call", &args.encode())
		.await
		.map_err(EvmContractError::Simulation)?;
	Simulation::decode(&output)
}

/// Submits `extrinsic` and waits until a finalized block dispatched it
/// successfully.
async fn submit_until_finalized(
	chain: &(impl TxSubmitter + BlockBackend),
	genesis: H256,
	extrinsic: &EncodedExtrinsic,
) -> Result<(), EvmContractError> {
	let mut progress = chain.submit_and_watch(genesis, extrinsic).await?;
	let finalized = loop {
		let event = progress.next().await.unwrap_or_else(|| {
			WatchEvent::Error("the watch ended without a terminal event".to_string())
		});
		match event {
			WatchEvent::InFinalizedBlock(hash) => break hash,
			event if event.is_terminal() => return Err(EvmContractError::NotFinalized(event)),
			_ => {},
		}
	};
	let number = chain
		.block_number(genesis, finalized)
		.await?
		.ok_or(EvmContractError::OutcomeMissing)?;
	let at = HashAndNumber { hash: finalized, number };
	match chain.dispatch_outcome(genesis, at, extrinsic.hash()).await? {
		Some(DispatchOutcome::Succeeded) => Ok(()),
		Some(DispatchOutcome::Failed) => Err(EvmContractError::DispatchFailed),
		None => Err(EvmContractError::OutcomeMissing),
	}
}

/// `sp_weights::Weight`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Encode, Decode, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Weight {
	/// Computation time.
	#[codec(compact)]
	pub ref_time: u64,
	/// Proof size in bytes.
	#[codec(compact)]
	pub proof_size: u64,
}

/// `pallet_revive::StorageDeposit` over a `u128` balance.
#[derive(Decode)]
#[cfg_attr(test, derive(Encode))]
pub enum StorageDeposit {
	/// Balance returned to the origin.
	Refund {
		/// Amount refunded.
		_amount: u128,
	},
	/// Balance charged to the origin.
	Charge(u128),
}

impl StorageDeposit {
	/// The balance charged to the origin; a refund charges nothing.
	fn charged(&self) -> u128 {
		match self {
			Self::Charge(amount) => *amount,
			Self::Refund { .. } => 0,
		}
	}
}

/// `pallet_revive::ExecReturnValue`.
#[derive(Decode)]
#[cfg_attr(test, derive(Encode))]
pub struct ExecReturnValue {
	/// `ReturnFlags` bits; [`REVERT_FLAG`] marks a revert.
	pub flags: u32,
	/// The contract's return data.
	pub data: Vec<u8>,
}

/// `ReturnFlags::REVERT`.
const REVERT_FLAG: u32 = 1;

/// A `DispatchError` kept as its raw bytes, since naming the error needs the
/// runtime's metadata. It is the last field of a `ContractResult`, so it takes
/// the rest of the input.
pub struct RawDispatchError(
	/// Encoded runtime error.
	pub Vec<u8>,
);

impl Decode for RawDispatchError {
	fn decode<I: Input>(input: &mut I) -> Result<Self, parity_scale_codec::Error> {
		let len = input.remaining_len()?.ok_or("DispatchError of unknown length")?;
		let mut bytes = vec![0; len];
		input.read(&mut bytes)?;
		Ok(Self(bytes))
	}
}

#[cfg(test)]
impl Encode for RawDispatchError {
	fn encode_to<O: parity_scale_codec::Output + ?Sized>(&self, dest: &mut O) {
		dest.write(&self.0);
	}
}

/// `pallet_revive::ContractResult<ExecReturnValue, u128>`.
#[derive(Decode)]
#[cfg_attr(test, derive(Encode))]
pub struct ContractResult {
	/// Weight the call consumed.
	pub _weight_consumed: Weight,
	/// Weight the call needs as its limit.
	pub weight_required: Weight,
	/// Storage deposit the call charged or refunded.
	pub storage_deposit: StorageDeposit,
	/// Largest storage deposit the call needed while it ran.
	pub max_storage_deposit: StorageDeposit,
	/// Gas the call consumed.
	pub _gas_consumed: u128,
	/// What the contract returned, or why the runtime rejected the call.
	pub result: Result<ExecReturnValue, RawDispatchError>,
}

/// Arguments of the `ReviveApi_call` runtime API.
#[derive(Default, Encode)]
struct ReviveApiCallArgs<'a> {
	/// Account the call is simulated from.
	origin: [u8; 32],
	/// Contract the call is sent to.
	dest: [u8; 20],
	/// Balance sent with the call.
	value: u128,
	/// Weight limit; `None` leaves it for the node to estimate.
	gas_limit: Option<Weight>,
	/// Storage deposit limit; `None` leaves it for the node to estimate.
	storage_deposit_limit: Option<u128>,
	/// ABI-encoded call data.
	input_data: &'a [u8],
}

/// What a contract call would do if its origin sent it now: its return data
/// and the limits a real call must declare.
#[derive(Debug, PartialEq, Eq)]
struct Simulation {
	/// The contract's return data.
	output: Vec<u8>,
	/// Weight the call needs as its limit.
	weight_required: Weight,
	/// Peak storage charge, including deposits the call later refunds.
	storage_deposit: u128,
}

impl Simulation {
	/// Decodes a `ReviveApi_call` result, rejecting dispatch failures and reverts.
	fn decode(output: &[u8]) -> Result<Self, EvmContractError> {
		let ContractResult {
			weight_required, storage_deposit, max_storage_deposit, result, ..
		} = ContractResult::decode(&mut &output[..]).map_err(EvmContractError::Malformed)?;
		let output = match result {
			Ok(ExecReturnValue { flags, data }) if flags & REVERT_FLAG != 0 => {
				return Err(EvmContractError::Reverted { detail: describe_revert(&data) });
			},
			Ok(ExecReturnValue { data, .. }) => data,
			Err(RawDispatchError(error)) => {
				return Err(EvmContractError::Dispatch {
					detail: format!("0x{}", hex::encode(error)),
				});
			},
		};
		Ok(Self {
			output,
			weight_required,
			storage_deposit: storage_deposit.charged().max(max_storage_deposit.charged()),
		})
	}
}

/// Arguments of the `Revive.call` extrinsic.
#[derive(Debug, Default, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct ReviveCallArgs {
	dest: [u8; 20],
	value: u128,
	weight_limit: Weight,
	storage_deposit_limit: u128,
	data: Vec<u8>,
}

/// Renders a Solidity revert payload: `Error(string)` and `Panic(uint256)` by
/// their meaning, anything else as hex. The reason is escaped because any
/// contract, such as a name owner's resolver, chooses it.
fn describe_revert(data: &[u8]) -> String {
	if data.is_empty() {
		return "(empty)".to_string();
	}
	GenericContractError::abi_decode(data)
		.map(|error| error.to_string().escape_debug().to_string())
		.unwrap_or_else(|_| format!("0x{}", hex::encode(data)))
}

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::Compact;
	use subxt::{ext::frame_decode::extrinsics::encode_call_data, metadata::Metadata};

	const ASSET_HUB_METADATA: &[u8] =
		include_bytes!("../../tests/fixtures/paseo-next-asset-hub-3004000-metadata.scale");

	#[test]
	fn a_contract_write_uses_required_weight_and_peak_storage_deposit() {
		let output = ContractResult {
			_weight_consumed: Weight { ref_time: 10, proof_size: 20 },
			weight_required: Weight { ref_time: 30, proof_size: 40 },
			storage_deposit: StorageDeposit::Refund { _amount: 50 },
			max_storage_deposit: StorageDeposit::Charge(60),
			_gas_consumed: 70,
			result: Ok(ExecReturnValue { flags: 0, data: vec![80] }),
		}
		.encode();

		assert_eq!(
			Simulation::decode(&output).unwrap(),
			Simulation {
				output: vec![80],
				weight_required: Weight { ref_time: 30, proof_size: 40 },
				storage_deposit: 60,
			}
		);
	}

	#[test]
	fn a_contract_call_encodes_its_limits_as_asset_hub_metadata_expects() {
		let metadata = Metadata::decode(&mut &ASSET_HUB_METADATA[..]).unwrap();
		let args = ReviveCallArgs {
			dest: [0x44; 20],
			weight_limit: Weight { ref_time: 456_789, proof_size: 12_345 },
			storage_deposit_limit: 987_654_321,
			data: vec![1, 2, 3],
			..Default::default()
		};

		let encoded =
			encode_call_data(REVIVE_PALLET, "call", &args, &metadata, metadata.types()).unwrap();

		let pallet = metadata.pallet_by_name(REVIVE_PALLET).unwrap();
		assert_eq!(
			encoded,
			(
				pallet.call_index(),
				pallet.call_variant_by_name("call").unwrap().index,
				[0x44u8; 20],
				Compact(0u128),
				Compact(456_789u64),
				Compact(12_345u64),
				Compact(987_654_321u128),
				vec![1u8, 2, 3],
			)
				.encode()
		);
	}
}
