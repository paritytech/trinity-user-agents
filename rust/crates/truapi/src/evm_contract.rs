//! Contract reads and prepared writes over pallet-revive.

use alloy_sol_types::{GenericContractError, SolInterface};
use futures::StreamExt;
use parity_scale_codec::{Decode, Encode, Input};
use subxt::{ext::scale_encode::EncodeAsType, utils::H256};
use thiserror::Error;

use crate::{
	chain::{
		BlockBackend, DispatchOutcome, EncodedExtrinsic, ExtrinsicBuildError, ExtrinsicService,
		HashAndNumber, MortalExtrinsic, RuntimeApiCaller, SubstrateSigner, TxSubmitter, WatchEvent,
	},
	chain_runtime::{ChainRuntime, RuntimeFailure, encode_hex},
};

const REVIVE_PALLET: &str = "Revive";

// Eth-derived origins are implicitly mapped; a nonzero address also passes
// contracts that reject address(0).
const VIEW_CALL_ORIGIN: [u8; 32] = [0xEE; 32];

/// Why a call through [`EvmContractCaller`] failed.
#[derive(Debug, Error)]
pub enum EvmContractError {
	/// The chain could not be reached or failed a request before anything
	/// was submitted.
	#[error(transparent)]
	Chain(#[from] RuntimeFailure),
	/// The `ReviveApi_call` request failed.
	#[error("ReviveApi_call: {0}")]
	Simulation(#[source] Box<subxt::Error>),
	/// The `ReviveApi_call` output is not a `ContractResult`. The cause is
	/// kept out of the message, which SCALE renders across several lines.
	#[error("ReviveApi_call output is not a ContractResult")]
	Malformed(parity_scale_codec::Error),
	/// The call ended in a `DispatchError`: the runtime rejected it, or the
	/// contract trapped or exhausted a limit. A revert is [`Self::Reverted`].
	#[error("ReviveApi_call dispatch failed: {detail}")]
	Dispatch {
		/// Raw hex of the `DispatchError` payload.
		detail: String,
	},
	/// The contract executed but reverted in a read or simulation.
	#[error("contract reverted: {detail}")]
	Reverted {
		/// Rendered revert reason.
		detail: String,
	},
	/// The signed extrinsic could not be built, so nothing was submitted.
	#[error(transparent)]
	Build(#[from] ExtrinsicBuildError),
	/// Sending failed or the watch ended before a finalized block included
	/// the extrinsic. The node may have received it, so a block can still
	/// include it until its era ends: follow `extrinsic` through
	/// [`Mortality::death`](crate::chain::Mortality::death) before signing the
	/// write again. Resubmitting the same `extrinsic` is safe, since its
	/// nonce lets the chain include it at most once; signing a new one is not.
	#[error("extrinsic {hash:?} was not finalized: {event:?}", hash = .extrinsic.extrinsic.hash())]
	NotFinalized {
		/// The submitted extrinsic and its era.
		extrinsic: Box<MortalExtrinsic>,
		/// The event that ended the watch, [`WatchEvent::Error`] when sending
		/// or watching failed.
		event: WatchEvent,
	},
	/// The watch reported the extrinsic finalized in `block`, but how it
	/// dispatched could not be read there. Do not sign the write again: read
	/// its outcome at `block`, or, when `block` does not contain the
	/// extrinsic, follow `extrinsic` until its era ends as for
	/// [`Self::NotFinalized`].
	#[error(
		"the outcome of extrinsic {hash:?} in finalized block {block:?} is unreadable: {reason}",
		hash = .extrinsic.extrinsic.hash()
	)]
	OutcomeUnreadable {
		/// The submitted extrinsic.
		extrinsic: Box<MortalExtrinsic>,
		/// The finalized block the watch reported.
		block: H256,
		/// Why the outcome could not be read.
		reason: Box<RuntimeFailure>,
	},
	/// A finalized block included the extrinsic, which failed to dispatch.
	/// The call had no effect apart from its fee, so the write may be
	/// prepared again. A contract that reverts during execution ends here
	/// too, as `Revive::ContractReverted`, without its revert reason.
	#[error("the extrinsic failed to dispatch: {detail}")]
	DispatchFailed {
		/// The dispatch error as [`DispatchOutcome::Failed`] reports it: named
		/// by the block's runtime metadata, or
		/// `undecodable dispatch error 0x<fields>: <cause>` when it does not
		/// decode.
		detail: String,
	},
}

/// A simulated contract write bound to its signer and to the caller that
/// simulated it.
///
/// The request owns the calldata and limits. Its nonce, mortal era and
/// signature are selected when it is executed.
pub struct PreparedEvmTransaction<'a> {
	caller: &'a EvmContractCaller,
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
	/// Writes rely on the runtime's `Revive.AutoMap`: the simulation maps its
	/// origin, so it cannot detect an unmapped signer on a runtime without
	/// `AutoMap`.
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
				value: 0,
				weight_limit: simulation.weight_required,
				storage_deposit_limit: simulation.storage_deposit,
				data: input.to_vec(),
			},
		})
	}
}

impl PreparedEvmTransaction<'_> {
	/// Consumes the prepared call and waits for successful finalized dispatch.
	///
	/// The extrinsic gets a fresh nonce and mortal era and the prepared call's
	/// limits unchanged. The signer must keep the account it had at
	/// [`EvmContractCaller::dry_run`]. An error that leaves the write possibly
	/// on chain carries the submitted extrinsic, as
	/// [`EvmContractError::NotFinalized`] or
	/// [`EvmContractError::OutcomeUnreadable`].
	pub async fn execute_until_finalized(self) -> Result<(), EvmContractError> {
		let Self { caller, signer, call } = self;
		let extrinsic = caller
			.extrinsics
			.build_mortal_extrinsic(caller.genesis)
			.with_call_args(REVIVE_PALLET, "call", call)
			.with_signer(signer)
			.build()
			.await?;
		submit_until_finalized(&caller.chain, caller.genesis, extrinsic).await
	}
}

/// Reads one contract for every ABI-encoded input.
///
/// Returns ABI data or a transport, dispatch, revert or decoding error for each input,
/// in input order. The runtime transport determines concurrency and the block context.
/// The batch completes before returning an owned iterator that decodes its results.
pub async fn read_batch<Runtime: RuntimeApiCaller>(
	runtime: &mut Runtime,
	contract: &[u8; 20],
	inputs: Vec<Vec<u8>>,
) -> impl Iterator<Item = Result<Vec<u8>, EvmContractError>> + use<Runtime> {
	let calls = inputs
		.iter()
		.map(|input| ReviveApiCallArgs::new(&VIEW_CALL_ORIGIN, contract, input).encode())
		.collect();
	runtime.call_batch("ReviveApi_call", calls).await.into_iter().map(|output| {
		let output = output.map_err(|error| EvmContractError::Simulation(error.into()))?;
		Ok(Simulation::decode(&output)?.output)
	})
}

async fn simulate(
	runtime: &mut impl RuntimeApiCaller,
	origin: &[u8; 32],
	contract: &[u8; 20],
	input: &[u8],
) -> Result<Simulation, EvmContractError> {
	let args = ReviveApiCallArgs::new(origin, contract, input);
	let output = runtime
		.call("ReviveApi_call", &args.encode())
		.await
		.map_err(|error| EvmContractError::Simulation(error.into()))?;
	Simulation::decode(&output)
}

/// Submits `extrinsic` and waits until a finalized block dispatched it
/// successfully.
async fn submit_until_finalized(
	chain: &(impl TxSubmitter + BlockBackend),
	genesis: H256,
	extrinsic: MortalExtrinsic,
) -> Result<(), EvmContractError> {
	let extrinsic = Box::new(extrinsic);
	let block = match finalized_inclusion(chain, genesis, &extrinsic.extrinsic).await {
		Ok(block) => block,
		Err(event) => return Err(EvmContractError::NotFinalized { extrinsic, event }),
	};
	match finalized_outcome(chain, genesis, block, &extrinsic.extrinsic).await {
		Ok(DispatchOutcome::Succeeded) => Ok(()),
		Ok(DispatchOutcome::Failed(detail)) => Err(EvmContractError::DispatchFailed { detail }),
		Err(reason) => {
			Err(EvmContractError::OutcomeUnreadable { extrinsic, block, reason: Box::new(reason) })
		},
	}
}

/// Sends `extrinsic` and returns the finalized block that includes it, or
/// the event that ended the watch without one.
async fn finalized_inclusion(
	chain: &impl TxSubmitter,
	genesis: H256,
	extrinsic: &EncodedExtrinsic,
) -> Result<H256, WatchEvent> {
	let mut progress = chain
		.submit_and_watch(genesis, extrinsic)
		.await
		.map_err(|failure| WatchEvent::Error(failure.to_string()))?;
	loop {
		let event = progress.next().await.unwrap_or_else(|| {
			WatchEvent::Error("the watch ended without a terminal event".to_string())
		});
		match event {
			WatchEvent::InFinalizedBlock(hash) => return Ok(hash),
			event if event.is_terminal() => return Err(event),
			_ => {},
		}
	}
}

/// How `extrinsic` dispatched in the finalized `block`.
async fn finalized_outcome(
	chain: &impl BlockBackend,
	genesis: H256,
	block: H256,
	extrinsic: &EncodedExtrinsic,
) -> Result<DispatchOutcome, RuntimeFailure> {
	let number = chain.block_number(genesis, block).await?.ok_or_else(|| {
		RuntimeFailure::host_failure("block_number", "the node does not know the finalized block")
	})?;
	let at = HashAndNumber { hash: block, number };
	chain.dispatch_outcome(genesis, at, extrinsic.hash()).await?.ok_or_else(|| {
		RuntimeFailure::host_failure("dispatch_outcome", "the finalized block lacks the extrinsic")
	})
}

/// `sp_weights::Weight`.
#[derive(Encode, Decode, EncodeAsType)]
#[cfg_attr(test, derive(Default))]
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
#[derive(Encode)]
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

impl<'a> ReviveApiCallArgs<'a> {
	fn new(origin: &[u8; 32], contract: &[u8; 20], input: &'a [u8]) -> Self {
		Self {
			origin: *origin,
			dest: *contract,
			value: 0,
			gas_limit: None,
			storage_deposit_limit: None,
			input_data: input,
		}
	}
}

/// What a contract call would do if its origin sent it now: its return data
/// and the limits a real call must declare.
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
				return Err(EvmContractError::Dispatch { detail: encode_hex(&error) });
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
#[derive(EncodeAsType)]
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
		.unwrap_or_else(|_| encode_hex(data))
}
