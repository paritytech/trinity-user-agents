//! E3 · Operation start: an operation from request to registration.
//!
//! Selection, token resolution and planning run in the plan callback, inside
//! the registration's store transaction, so selection reads exactly the
//! state the registration commits against (invariant 1). The registration
//! callback then writes C1's rows with the records' identities. Nothing is
//! built here: C2's rebuild policy builds every attempt later.

use std::sync::{Arc, Mutex};

use parity_scale_codec::{Decode, Encode};
use rusqlite::Transaction;

use super::{
	Asset, ChainParams, CoinSecret, Exponent, HeldBackSpend, Initiator,
	ledger::{self, TokenId},
	namespace::coin_key,
	operation::{self, Begun, OpKey, OpKind, Retry},
	planning::{Call, Destination, Plan, PlanInput, Planned, account_of, plan, tokens_needed},
	policy::effective,
	registration::{Admission, RegisterError, Rows, coinage_domain, register, registry_domain},
	selection::{Forms, SelectError, SelectRequest, Want, select},
	tokens::{resolve, supply},
};
use crate::{
	durable::{DurableRequest, DurableTxEngine, DurableTxId, PolicyParams, RegistrationError},
	store::DbError,
};

/// The version of the policy-parameter layout this release writes.
pub const PARAMS_VERSION: u8 = 1;

/// A record's policy parameters: its call and its retry policy, after the
/// layout's version byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct RecordParams {
	/// The call the record makes.
	pub call: Call,
	/// When a failed attempt is built again.
	pub retry: Retry,
}

impl RecordParams {
	/// The SCALE encoding, after the version byte.
	pub fn to_params(self) -> PolicyParams {
		PolicyParams((PARAMS_VERSION, self).encode())
	}

	/// The parameters `params` holds, or none when they do not decode.
	pub fn from_params(params: &PolicyParams) -> Option<Self> {
		let (version, decoded) = <(u8, Self)>::decode(&mut params.0.as_slice()).ok()?;
		(version == PARAMS_VERSION).then_some(decoded)
	}
}

/// What to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
	/// B3 chooses the inputs.
	Select {
		/// The purse, already resolved.
		from: super::PurseId,
		/// How many cents.
		amount: u64,
		/// Where they go.
		to: Destination,
		/// Who asked, for held-back spending; none for a rebalance.
		initiator: Option<Initiator>,
		/// The user confirmed spending held-back value.
		confirmed: bool,
	},
	/// The caller chose: a sweep, an offload phase, a claim, a load, the
	/// registry.
	Given(PlanInput),
}

/// A coin an export hands to its payee.
#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(uniffi::Record)]
pub struct ExportedCoin {
	/// The coin's account.
	pub account: [u8; 32],
	/// The secret controlling it.
	pub secret: CoinSecret,
	/// Its denomination.
	pub exponent: Exponent,
}

/// A started operation's records, and for an export its coins.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registered {
	/// The records, in plan order.
	pub records: Vec<DurableTxId>,
	/// For an export, the coins handed to the payee.
	pub exported: Vec<ExportedCoin>,
}

/// Why nothing was started. Nothing was written.
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum StartError {
	/// The purse does not cover the request (B3 invariant 4).
	#[display("insufficient funds: {available} cents available")]
	InsufficientFunds {
		/// What the purse may take now or later.
		available: u64,
	},
	/// The period's free tokens do not reach the plan's unloads (D1
	/// invariant 1).
	#[display("no unload token is free")]
	NoUnloadToken,
	/// C1 refused the rows: another registration took an input or token
	/// the caller chose first, or an input is disclosed (C1).
	#[display("the registration was refused: {_0}")]
	Refused(#[error(not(source))] RegisterError),
	/// The request has nothing to do.
	#[display("the request has nothing to do")]
	Nothing,
	/// The durability layer does not serve the coinage domains (§1).
	#[display("the durability layer does not serve the coinage domains")]
	NotServed,
	/// The key names an operation with a different kind or arguments (B4).
	#[display("the key names an operation with different arguments")]
	KeyReused,
	/// The store failed.
	Db(DbError),
}

/// What starting an operation runs on.
pub struct Launcher<'a> {
	/// The durability layer the records are registered with.
	pub engine: &'a Arc<DurableTxEngine>,
	/// The parameters of the last runtime check that passed.
	pub params: &'a ChainParams,
	/// The recovery phrase's entropy, which derives an export's coin keys.
	pub entropy: &'a [u8],
}

/// A new operation, recorded in B4 in the same store transaction that
/// registers its records.
pub struct Begin {
	/// What it does.
	pub kind: OpKind,
	/// The SCALE encoding of the arguments it was started with.
	pub arguments: Vec<u8>,
}

/// What [`begin_and_start`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launched {
	/// The key was new: the operation is recorded and its records registered.
	New(Registered),
	/// The key names an operation with the same kind and arguments; nothing
	/// was written.
	Existing,
}

/// Starts `request` as a further step of the operation `key`, which is
/// already recorded, whose records retry by `retry`.
pub async fn start(
	launcher: &Launcher<'_>,
	key: &OpKey,
	retry: Retry,
	request: Request,
	now: u64,
) -> Result<Registered, StartError> {
	match launch(launcher, key, None, retry, request, now).await? {
		Launched::New(registered) => Ok(registered),
		// Only a start that begins the operation can find it existing.
		Launched::Existing => Err(StartError::Nothing),
	}
}

/// Records the operation `key` and starts `request` as it, in one store
/// transaction, so a start that is refused records no operation; finds the
/// operation instead when `key` already names it (B4 `begin`).
pub async fn begin_and_start(
	launcher: &Launcher<'_>,
	key: &OpKey,
	begin: Begin,
	retry: Retry,
	request: Request,
	now: u64,
) -> Result<Launched, StartError> {
	launch(launcher, key, Some(begin), retry, request, now).await
}

async fn launch(
	launcher: &Launcher<'_>,
	key: &OpKey,
	begin: Option<Begin>,
	retry: Retry,
	request: Request,
	now: u64,
) -> Result<Launched, StartError> {
	let Launcher { engine, params, entropy } = launcher;
	let domain =
		if matches!(request, Request::Given(PlanInput::RegisterInstallation(_))) { registry_domain() } else { coinage_domain() };
	let prepared: Arc<Mutex<Option<Prepared>>> = Arc::new(Mutex::new(None));
	let planned_into = prepared.clone();
	let plan_params = (*params).clone();
	let plan_request = request.clone();
	let begun_key = key.clone();
	let durable_request: DurableRequest<Refusal> = DurableRequest::deferred_in(domain.clone(), Some(key.group()), move |tx| {
		if let Some(Begin { kind, arguments }) = &begin {
			match operation::begin(tx, &begun_key, *kind, arguments)? {
				Ok(Begun::New) => {},
				Ok(Begun::Existing) => return Err(Refusal::Existing),
				Err(_) => return Err(Refusal::KeyReused),
			}
		}
		let ready = prepare(tx, &plan_request, now, &plan_params)?;
		let records = ready.plan.transactions.iter().map(|planned| RecordParams { call: planned.call, retry }.to_params()).collect();
		*planned_into.lock().expect("the plan cell is not poisoned") = Some(ready);
		Ok(records)
	});
	let operation = key.clone();
	let registered = engine
		.execute_with(durable_request, move |tx, ids| {
			let ready = prepared.lock().expect("the plan cell is not poisoned").take().ok_or(Refusal::Nothing)?;
			let records: Vec<(DurableTxId, Rows)> = ids.iter().copied().zip(ready.rows.iter().cloned()).collect();
			register(tx, &domain, &operation, ready.admission, &records)?.map_err(Refusal::Register)?;
			if !ready.exported.is_empty() {
				operation::record_exported(tx, &operation, &ready.exported)?;
			}
			Ok(ready.exported)
		})
		.await;
	match registered {
		Ok((records, exported)) => {
			Ok(Launched::New(Registered { records, exported: exported_coins(entropy, &exported) }))
		},
		Err(RegistrationError::Hook(Refusal::Existing)) => Ok(Launched::Existing),
		Err(RegistrationError::Hook(refusal)) => Err(refusal.into()),
		Err(RegistrationError::Db(error)) => Err(StartError::Db(error)),
		Err(RegistrationError::UnknownDomain(_)) => Err(StartError::NotServed),
	}
}

/// Whether `request` needs the user's confirmation before it starts: its
/// initiator's setting is `AfterConfirmation`, it is not confirmed, and only
/// held-back value covers it (A4).
pub fn needs_confirmation(view: &ledger::Snapshot, request: &Request, now: u64, params: &ChainParams) -> bool {
	let Request::Select { from, amount, initiator: Some(initiator), confirmed: false, .. } = request else {
		return false;
	};
	let policy = effective(view.policy(), params);
	if policy.held_back_for(*initiator) != HeldBackSpend::AfterConfirmation {
		return false;
	}
	let base = select_request(*from, *amount, *initiator);
	let without = select(view, base, now, params);
	let with = select(view, SelectRequest { held_back: true, ..base }, now, params);
	matches!(without, Err(SelectError::InsufficientFunds { .. })) && with.is_ok()
}

/// What the plan callback hands to the registration callback.
struct Prepared {
	plan: Plan,
	rows: Vec<Rows>,
	admission: Admission,
	exported: Vec<(Asset, Exponent)>,
}

/// Why the registration was refused inside its store transaction.
#[derive(Debug, derive_more::From)]
enum Refusal {
	Select(SelectError),
	NoUnloadToken,
	Register(RegisterError),
	Nothing,
	Existing,
	KeyReused,
	Db(DbError),
}

impl From<rusqlite::Error> for Refusal {
	fn from(error: rusqlite::Error) -> Self {
		Self::Db(error.into())
	}
}

impl From<Refusal> for StartError {
	fn from(refusal: Refusal) -> Self {
		match refusal {
			Refusal::Select(SelectError::InsufficientFunds { available }) => Self::InsufficientFunds { available },
			Refusal::Select(SelectError::NoUnloadToken) | Refusal::NoUnloadToken => Self::NoUnloadToken,
			Refusal::Register(error) => Self::Refused(error),
			Refusal::Nothing | Refusal::Existing => Self::Nothing,
			Refusal::KeyReused => Self::KeyReused,
			Refusal::Db(error) => Self::Db(error),
		}
	}
}

/// Step 1: select, resolve tokens, plan, allocate the outputs, keep an
/// import's secrets and mark an export's coins.
fn prepare(
	tx: &Transaction<'_>,
	request: &Request,
	now: u64,
	params: &ChainParams,
) -> Result<Prepared, Refusal> {
	let view = ledger::snapshot(tx)?;
	let input = match request {
		Request::Given(input) => input.clone(),
		Request::Select { from, amount, to, initiator, confirmed } => {
			let base = select_request(*from, *amount, initiator.unwrap_or(Initiator::User));
			let base = SelectRequest { entries_first: *initiator == Some(Initiator::Product), ..base };
			let selection = match select(&view, base, now, params) {
				Err(SelectError::InsufficientFunds { .. }) if held_back_allowed(&view, *initiator, *confirmed, params) => {
					select(&view, SelectRequest { held_back: true, ..base }, now, params)?
				},
				selected => selected?,
			};
			PlanInput::Spend { selection, to: *to }
		},
	};
	let needed = tokens_needed(&input, &view, params);
	let tokens: Vec<TokenId> = resolve(needed, &supply(now, &view, params)).ok_or(Refusal::NoUnloadToken)?;
	let plan = plan(&input, &tokens, &view, params);
	if plan.transactions.is_empty() && plan.handed_out.is_empty() {
		return Err(Refusal::Nothing);
	}
	let mut rows = Vec::with_capacity(plan.transactions.len());
	let mut exported: Vec<(Asset, Exponent)> =
		plan.handed_out.iter().filter_map(|coin| Some((*coin, view.asset(coin)?.exponent))).collect();
	for planned in &plan.transactions {
		let outputs = allocate_outputs(tx, planned)?;
		if matches!(input, PlanInput::Spend { to: Destination::Export(_), .. }) {
			let delivered = planned.outputs.iter().zip(&outputs).filter(|(output, _)| output.delivered);
			exported.extend(delivered.map(|(output, asset)| (*asset, output.exponent)));
		}
		let tokens = planned.token.into_iter().collect();
		rows.push(Rows { inputs: planned.inputs.clone(), outputs, tokens });
	}
	if let PlanInput::Import { coins, .. } = &input {
		let secrets: Vec<([u8; 32], CoinSecret)> =
			coins.iter().filter_map(|secret| Some((account_of(secret)?, secret.clone()))).collect();
		ledger::keep_secrets(tx, &secrets)?;
	}
	if !exported.is_empty() {
		let coins: Vec<Asset> = exported.iter().map(|(coin, _)| *coin).collect();
		ledger::mark_disclosed(tx, &coins)?.map_err(|coin| Refusal::Register(RegisterError::InputTaken(coin)))?;
	}
	let admission = match input {
		PlanInput::Import { .. } => Admission::Import,
		PlanInput::Reclaim { .. } => Admission::Reclaim,
		_ => Admission::Ordinary,
	};
	Ok(Prepared { plan, rows, admission, exported })
}

fn select_request(from: super::PurseId, amount: u64, initiator: Initiator) -> SelectRequest {
	SelectRequest {
		purse: from,
		want: Want::Amount(amount),
		forms: Forms::Any,
		entries_first: initiator == Initiator::Product,
		held_back: false,
	}
}

fn held_back_allowed(view: &ledger::Snapshot, initiator: Option<Initiator>, confirmed: bool, params: &ChainParams) -> bool {
	let Some(initiator) = initiator else { return false };
	match effective(view.policy(), params).held_back_for(initiator) {
		HeldBackSpend::Always => true,
		HeldBackSpend::AfterConfirmation => confirmed,
		HeldBackSpend::Never => false,
	}
}

/// Fresh assets for a planned transaction's outputs, in order.
fn allocate_outputs(tx: &Transaction<'_>, planned: &Planned) -> Result<Vec<Asset>, DbError> {
	planned
		.outputs
		.iter()
		.map(|output| Ok(ledger::allocate(tx, output.purse, output.kind, &[output.exponent])?[0]))
		.collect()
}

/// The secrets of an export's coins, derived from the seed.
pub fn exported_coins(entropy: &[u8], coins: &[(Asset, Exponent)]) -> Vec<ExportedCoin> {
	coins
		.iter()
		.filter_map(|(coin, exponent)| {
			let Asset::Coin { purse, installation, index } = coin else { return None };
			let (secret, account) = coin_key(entropy, *purse, *installation, *index).ok()?;
			Some(ExportedCoin { account, secret, exponent: *exponent })
		})
		.collect()
}

#[cfg(test)]
mod tests;
