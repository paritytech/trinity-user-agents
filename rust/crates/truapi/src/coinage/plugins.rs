//! C2 · Durability plug-ins: the layer's two domains, and the completion
//! oracle and rebuild policy of each.
//!
//! The core adds both domains through [`with_coinage`] when it creates the
//! durability layer, which is before this layer starts. Building waits
//! behind [`Running`], which G3 opens at the end of its start sequence
//! (invariant 6). The oracle claims *not completed* only on facts that
//! cannot reverse, and only when no failed dispatch shows (invariants 2 and
//! 3).

use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
};

use parking_lot::RwLock;
use subxt::utils::H256;

use super::{
	Asset, ChainParams, CoinSecret, Snapshot,
	assembly::{self, Build, Context, Funding, Keys, Personhood, Ring, Transaction},
	chain::{AliasState, ChainCoin, CoinageChain, RingPosition},
	ledger::{self, Mark, Read, Subject},
	namespace::{RegistryKey, coin_key, datastore_key, entry_seed},
	operation::{OpKey, Retry},
	planning::Call,
	registration::{RecordFacts, coinage_domain, registry_domain},
	start::RecordParams,
	tokens::period_of,
};
use crate::{
	chain::{ExtrinsicService, Heads, MortalExtrinsic},
	chain_runtime::RuntimeFailure,
	durable::{
		self, CompletionOracle, DeferredDurableTx, DurableRegistry, DurableTxEntry, DurableTxId, DurableTxStatus,
		HeadKind, PassScope, PolicyError, PolicyParams, Preparation, SubmissionPolicy, Unobservable,
		VerdictFailureKind, Window,
	},
	runtime::{
		Vrf, load_vrf,
		statement_allowance::{PersonhoodSigner, ring::RingParams},
	},
	store::Db,
};

/// The state G1 and G3 hold in memory and share with the plug-ins: closed
/// until G3's start sequence has run, then the parameters of the last
/// runtime check that passed, whether the latest check failed, and the
/// funding capability of each top-up that has not ended.
#[derive(Clone, Default)]
pub struct Running(Arc<RwLock<Option<RunningState>>>);

/// What [`Running`] holds once open.
#[derive(Clone)]
pub struct RunningState {
	/// The parameters of the last runtime check that passed.
	pub params: ChainParams,
	/// Whether the latest runtime check failed.
	pub check_failed: bool,
	/// The funding capability of each top-up that has not ended.
	pub funding: HashMap<OpKey, Arc<dyn Funding>>,
}

impl Running {
	/// A closed handle.
	pub fn new() -> Self {
		Self::default()
	}

	/// Opens the handle with `state`; G3 calls it at the end of its start
	/// sequence.
	pub fn open(&self, state: RunningState) {
		*self.0.write() = Some(state);
	}

	/// Changes the open state; nothing happens while the handle is closed.
	pub fn update(&self, change: impl FnOnce(&mut RunningState)) {
		if let Some(state) = self.0.write().as_mut() {
			change(state);
		}
	}

	/// The open state, or none while closed.
	pub fn state(&self) -> Option<RunningState> {
		self.0.read().clone()
	}
}

/// The recovery phrase's entropy, which derives every key the layer uses.
pub trait Seed: Send + Sync {
	/// The entropy, or none while it is unavailable.
	fn entropy(&self) -> Option<Vec<u8>>;
}

/// The person whose free tokens pay for unloads.
#[async_trait::async_trait]
pub trait PersonhoodSource: Send + Sync {
	/// The person's keys and the ring that includes them, at a root
	/// revision, or none while unavailable.
	async fn person(&self) -> Option<(Arc<dyn PersonhoodSigner>, RingParams, u32)>;
}

impl<Source: Seed> Seed for std::sync::Weak<Source> {
	fn entropy(&self) -> Option<Vec<u8>> {
		self.upgrade()?.entropy()
	}
}

#[async_trait::async_trait]
impl<Source: PersonhoodSource> PersonhoodSource for std::sync::Weak<Source> {
	async fn person(&self) -> Option<(Arc<dyn PersonhoodSigner>, RingParams, u32)> {
		self.upgrade()?.person().await
	}
}

/// Builds the installation registry's appends (F4 invariant 4).
#[async_trait::async_trait]
pub trait RegistryWriter: Send + Sync {
	/// The signed call that appends `record` to the list of the `//datastore`
	/// account whose secret is `datastore`. Fails while it cannot be built
	/// yet, for example before the account can pay for it; the caller asks
	/// again later.
	async fn append(&self, datastore: &CoinSecret, record: &[u8]) -> Result<MortalExtrinsic, String>;
}

/// What the plug-ins run on: only what exists before the layer starts.
pub struct CoinageDeps {
	/// The shared store.
	pub store: Db,
	/// The recovery phrase's entropy.
	pub seed: Arc<dyn Seed>,
	/// The current time, in Unix seconds.
	pub clock: Arc<dyn Fn() -> u64 + Send + Sync>,
	/// Reads of the People chain, which runs the coinage pallet.
	pub people: Arc<dyn CoinageChain>,
	/// The People chain's genesis.
	pub people_genesis: H256,
	/// Asset Hub's genesis, which runs the installation registry.
	pub asset_hub_genesis: H256,
	/// Builds extrinsics.
	pub extrinsics: Arc<ExtrinsicService>,
	/// The person whose free tokens pay for unloads.
	pub personhood: Arc<dyn PersonhoodSource>,
	/// The handle G1 and G3 share with the plug-ins.
	pub running: Running,
	/// Builds the installation registry's appends; none when the host runs
	/// no registry, and then nothing waits for a registration.
	pub registry: Option<Arc<dyn RegistryWriter>>,
}

/// `list` with the layer's two domains: `coinage` on the People chain and
/// `coinage-registry` on Asset Hub.
pub fn with_coinage(list: DurableRegistry, deps: CoinageDeps) -> DurableRegistry {
	let shared = Arc::new(deps);
	let oracle = Arc::new(CoinageOracle(shared.clone()));
	let policy = Arc::new(CoinagePolicy(shared.clone()));
	list.with_domain(coinage_domain(), shared.people_genesis, oracle, policy)
		.with_domain(registry_domain(), shared.asset_hub_genesis, Arc::new(Unobservable), Arc::new(RegistryPolicy(shared.clone())))
}

/// The `coinage` domain's completion oracle.
struct CoinageOracle(Arc<CoinageDeps>);

/// The `coinage` domain's rebuild policy.
struct CoinagePolicy(Arc<CoinageDeps>);

/// The `coinage-registry` domain's rebuild policy: it builds each append of
/// the current installation's sealed record through the registry writer
/// (F4 invariant 4), once the start sequence has run.
struct RegistryPolicy(Arc<CoinageDeps>);

#[async_trait::async_trait]
impl SubmissionPolicy for RegistryPolicy {
	fn can_retry(&self, _: &DurableTxEntry, params: &PolicyParams, failure: VerdictFailureKind) -> bool {
		RecordParams::from_params(params).is_some_and(|params| retries(params, failure, (self.0.clock)()))
	}

	async fn prepare(&self, batch: &[DeferredDurableTx]) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let deps = &self.0;
		let Some(registry) = &deps.registry else { return Ok(HashMap::new()) };
		if deps.running.state().is_none() {
			return Ok(HashMap::new());
		}
		let Some(entropy) = deps.seed.entropy() else { return Ok(HashMap::new()) };
		let (datastore, _) = datastore_key(&entropy).map_err(policy_error)?;
		let key = RegistryKey::of(&datastore).ok_or_else(|| policy_error("the datastore secret is not a key"))?;
		let mut answers = HashMap::new();
		for waiting in batch {
			let call = RecordParams::from_params(&waiting.plan.params).map(|params| params.call);
			let Some(Call::AppendRegistryRecord(installation)) = call else {
				answers.insert(waiting.id, Preparation::GiveUp);
				continue;
			};
			let extrinsic = registry.append(&datastore, &key.seal(installation)).await.map_err(policy_error)?;
			answers.insert(waiting.id, Preparation::Ready { extrinsic, params: None });
		}
		Ok(answers)
	}
}

/// Whether a record whose parameters are `params` is built again after
/// `failure` at `now`, in Unix seconds (invariant 3): never after a
/// dispatch failure of a token-paid unload, whose token that attempt spent;
/// otherwise as its retry policy says. Under `Until`, an expiry is retried
/// at any time, while its inputs last, and any other failure only until
/// the deadline, since one that repeats on the same inputs would otherwise
/// be built again without end.
fn retries(params: RecordParams, failure: VerdictFailureKind, now: u64) -> bool {
	let token_spent = matches!(params.call, Call::UnloadToCoins | Call::UnloadToExternal(_)) &&
		failure == VerdictFailureKind::DispatchFailed;
	if token_spent {
		return false;
	}
	match params.retry {
		Retry::Never => false,
		Retry::ExpiryOnly => failure == VerdictFailureKind::Expired,
		Retry::Until(deadline) => failure == VerdictFailureKind::Expired || now <= deadline,
	}
}

#[async_trait::async_trait]
impl SubmissionPolicy for CoinagePolicy {
	fn can_retry(&self, _: &DurableTxEntry, params: &PolicyParams, failure: VerdictFailureKind) -> bool {
		RecordParams::from_params(params).is_some_and(|params| retries(params, failure, (self.0.clock)()))
	}

	async fn prepare(&self, batch: &[DeferredDurableTx]) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let deps = &self.0;
		let Some(running) = deps.running.state() else { return Ok(HashMap::new()) };
		if running.check_failed {
			return Ok(HashMap::new());
		}
		let Some(entropy) = deps.seed.entropy() else { return Ok(HashMap::new()) };
		let view = deps.store.read(ledger::snapshot).await.map_err(policy_error)?;
		let now = (deps.clock)();
		if deps.registry.is_some() && !view.installation_registered() {
			return Ok(HashMap::new());
		}
		let round = BuildRound {
			view,
			heads: deps.people.heads().await.map_err(policy_error)?,
			vrf: load_vrf().await.map_err(|error| policy_error(format!("{error:?}")))?,
			running,
			entropy,
			now,
		};
		let mut answers = HashMap::new();
		for waiting in batch {
			if let Some(answer) = self.decide(waiting, &round).await {
				answers.insert(waiting.id, answer);
			}
		}
		Ok(answers)
	}
}

/// What one round of building reads once, for every record of its batch.
struct BuildRound {
	view: Snapshot,
	heads: Heads,
	vrf: Vrf,
	running: RunningState,
	entropy: Vec<u8>,
	now: u64,
}

impl CoinagePolicy {
	/// One record's answer, or none while it waits (invariant 5).
	async fn decide(&self, waiting: &DeferredDurableTx, round: &BuildRound) -> Option<Preparation> {
		let deps = &self.0;
		let BuildRound { view, heads, vrf, running, entropy, now } = round;
		let now = *now;
		let Some(params) = RecordParams::from_params(&waiting.plan.params) else { return Some(Preparation::GiveUp) };
		let facts = view.registrations().record(waiting.id)?;
		let cancelled =
			view.operation(&facts.operation).is_some_and(|operation| operation.cancel_requested || operation.released);
		let past_deadline = matches!(params.retry, Retry::Until(deadline) if now > deadline);
		let inputs_gone = matches!(params.call, Call::Load(_)) || facts.rows.inputs.iter().any(|input| gone(input, view));
		if cancelled || (past_deadline && inputs_gone) {
			return Some(Preparation::GiveUp);
		}
		let uncommitted_export = facts.rows.outputs.iter().any(|output| view.mark(output) == Some(Mark::Uncommitted));
		if uncommitted_export || !facts.rows.inputs.iter().all(|input| buildable(input, view, now)) {
			return None;
		}
		let tx = transaction(facts, params.call, view)?;
		let mut context = Context::default();
		let person = match params.call {
			Call::UnloadToCoins | Call::UnloadToExternal(_) => {
				let (exponent, ring, revision) = ring_of(view, facts.rows.inputs.first()?)?;
				let members = deps.people.ring_members_at(heads.finalized.hash, exponent, ring).await.ok()?;
				match alias_check(deps, facts, (exponent, ring), round).await? {
					AliasCheck::Clear => {},
					AliasCheck::Consumed => return Some(Preparation::GiveUp),
				}
				context.ring = Some(Ring { index: ring, revision, members });
				Some(deps.personhood.person().await?)
			},
			_ => None,
		};
		let funding = match params.call {
			Call::Load(from) => {
				let funding = running.funding.get(&facts.operation).cloned();
				let Some(funding) = funding else { return Some(Preparation::GiveUp) };
				if earlier_load_open(view, waiting.id, from) {
					return None;
				}
				let funds = deps.people.funds_at(heads.finalized.hash, from).await.ok()?;
				if funds < load_cost(&tx, &running.params) {
					return None;
				}
				context.nonce = Some(deps.people.nonce_at(heads.finalized.hash, from).await.ok()?);
				Some(funding)
			},
			_ => None,
		};
		let imported = match facts.rows.inputs.iter().find_map(|input| match input {
			Asset::PeerCoin(account) => Some(*account),
			_ => None,
		}) {
			Some(account) => Some(deps.store.read(move |conn| ledger::secret(conn, &account)).await.ok()??),
			None => None,
		};
		let keys = Keys {
			entropy,
			vrf,
			imported: imported.as_ref(),
			funding,
			person: person.as_ref().map(|(signer, ring, revision)| Personhood { signer: signer.as_ref(), ring, revision: *revision }),
		};
		let build = Build {
			service: &deps.extrinsics,
			genesis: deps.people_genesis,
			birth: heads.finalized,
			period: window_period(&running.params, params.retry, now),
		};
		match assembly::assemble(&build, &tx, &context, &keys, &running.params).await {
			Ok(Some(extrinsic)) => Some(Preparation::Ready { extrinsic, params: None }),
			Ok(None) => Some(Preparation::GiveUp),
			Err(_) => None,
		}
	}
}

/// The period of an attempt's window: A3's longest window, halved until it
/// ends before an `Until` deadline, and no longer than 4096 blocks, so that
/// every birth block is allowed (section 1).
fn window_period(params: &ChainParams, retry: Retry, now: u64) -> u32 {
	let block_time = params.config().block_time.as_secs().max(1);
	let mut period = params.config().longest_window.min(4096);
	if let Retry::Until(deadline) = retry {
		while period > 4 && now + u64::from(period) * block_time > deadline {
			period /= 2;
		}
	}
	period
}

/// Whether a load paid by `from` and registered before `record` has no
/// stable verdict yet: the layer builds one transaction per signing account
/// at a time, in registration order, so each reads the nonce the one
/// before it left (E2 invariant 4).
fn earlier_load_open(view: &Snapshot, record: DurableTxId, from: [u8; 32]) -> bool {
	view.registrations().records().any(|earlier| {
		let paid_by_from = earlier
			.state
			.plan
			.as_ref()
			.and_then(|plan| RecordParams::from_params(&plan.params))
			.is_some_and(|params| params.call == Call::Load(from));
		let stable = matches!(earlier.state.status, DurableTxStatus::FinalizedSuccess { .. } | DurableTxStatus::Failure(_));
		earlier.record < record && paid_by_from && !stable
	})
}

/// What the funding account must hold before a load is built: the value it
/// loads in the external asset's units. A load may empty the account, as
/// the shipped wallet's loads do.
fn load_cost(tx: &Transaction, params: &ChainParams) -> u128 {
	let cents: u64 = tx.outputs.iter().map(|(_, exponent)| exponent.value()).sum();
	u128::from(cents).saturating_mul(params.instance().asset_unit)
}

/// What the chain shows of an unload's aliases right before it is built.
enum AliasCheck {
	/// None is unloaded or locked.
	Clear,
	/// One is unloaded already: its entry was consumed elsewhere.
	Consumed,
}

/// Checks the aliases an unload would present, at the finalized head: the
/// chain checks them only at dispatch, where a failure would still spend the
/// unload's free token (§1). None, so the record waits, while one is locked
/// or the read fails.
async fn alias_check(
	deps: &CoinageDeps,
	facts: &RecordFacts,
	(exponent, ring): (i8, u32),
	round: &BuildRound,
) -> Option<AliasCheck> {
	let aliases: Vec<[u8; 32]> = facts
		.rows
		.inputs
		.iter()
		.filter_map(|input| match input {
			Asset::Entry { purse, installation, index } => {
				let seed = entry_seed(&round.entropy, *purse, *installation, *index);
				round.vrf.alias(&seed.0, &assembly::UNLOADING_RECYCLER_CONTEXT).ok()
			},
			_ => None,
		})
		.collect();
	let states = deps.people.aliases_at(round.heads.finalized.hash, exponent, ring, &aliases).await.ok()?;
	if states.iter().any(|state| matches!(state, Some(AliasState::Unloaded))) {
		return Some(AliasCheck::Consumed);
	}
	let locked = states.iter().any(|state| matches!(state, Some(AliasState::Locked(until)) if *until > round.now));
	(!locked).then_some(AliasCheck::Clear)
}

/// Whether an input is usable for building now, ignoring that this record
/// itself holds it: a coin present on chain and not locked, an entry whose
/// key its ring's root includes and whose alias is not locked.
fn buildable(input: &Asset, view: &Snapshot, now: u64) -> bool {
	let Some(value) = view.observation(&Subject::Asset(*input)).and_then(|observation| observation.value.as_ref()) else {
		return false;
	};
	match value {
		Read::Coin { now: Some(coin), .. } => coin.locked_until.is_none_or(|until| until <= now),
		Read::Entry(Some(entry)) => entry.locked_until.is_none_or(|until| until <= now) && entry.included > 0,
		_ => false,
	}
}

/// Whether F1 last saw `input` gone: a coin whose account it read empty. An
/// input it never read is not gone.
fn gone(input: &Asset, view: &Snapshot) -> bool {
	let value = view.observation(&Subject::Asset(*input)).and_then(|observation| observation.value.as_ref());
	matches!(value, Some(Read::Coin { now: None, .. }))
}

/// The transaction a record builds, from C1's rows and its call.
fn transaction(facts: &RecordFacts, call: Call, view: &Snapshot) -> Option<Transaction> {
	let exponent = |asset: &Asset| match asset {
		Asset::PeerCoin(_) => match view.observation(&Subject::Asset(*asset))?.value.as_ref()? {
			Read::Coin { now: Some(coin), .. } => Some(coin.exponent),
			_ => None,
		},
		_ => view.asset(asset).map(|record| record.exponent),
	};
	let pair = |asset: &Asset| Some((*asset, exponent(asset)?));
	Some(Transaction {
		call,
		inputs: facts.rows.inputs.iter().map(pair).collect::<Option<_>>()?,
		outputs: facts.rows.outputs.iter().map(pair).collect::<Option<_>>()?,
		token: facts.rows.tokens.first().copied(),
	})
}

/// An entry's exponent, as the chain stores it, and the ring and revision it
/// was last observed in.
fn ring_of(view: &Snapshot, entry: &Asset) -> Option<(i8, u32, u32)> {
	let exponent = i8::try_from(view.asset(entry)?.exponent.get()).ok()?;
	match view.observation(&Subject::Asset(*entry))?.value.as_ref()? {
		Read::Entry(Some(place)) => Some((exponent, place.ring, place.revision)),
		_ => None,
	}
}

fn policy_error(error: impl core::fmt::Display) -> PolicyError {
	PolicyError { reason: error.to_string() }
}

#[async_trait::async_trait]
impl CompletionOracle for CoinageOracle {
	async fn open_pass(
		&self,
		transactions: &[DurableTxEntry],
		heads: &Heads,
	) -> Result<Box<dyn PassScope>, RuntimeFailure> {
		let deps = &self.0;
		let failure = |reason: String| RuntimeFailure::host_failure("coinage_open_pass", reason);
		let view = deps.store.read(ledger::snapshot).await.map_err(|error| failure(error.to_string()))?;
		let entropy = deps.seed.entropy().ok_or_else(|| failure("the seed is unavailable".into()))?;
		let vrf = load_vrf().await.map_err(|error| failure(format!("{error:?}")))?;
		let now = (deps.clock)();
		let records: Vec<&RecordFacts> =
			transactions.iter().filter_map(|entry| view.registrations().record(entry.id)).collect();
		let windows = minter_windows(&deps.store, &records, &view).await;
		let keys = AssetKeys::derive(&records, &view, &entropy, &vrf);
		let finalized = Reads::at(deps.people.as_ref(), heads.finalized.hash, &keys).await;
		let best = Reads::at(deps.people.as_ref(), heads.best.hash, &keys).await;
		let token_guard = token_guards(deps, &records, &view, now, heads).await;
		let mut proven = HashMap::new();
		for facts in records {
			for (head, reads) in [(HeadKind::Finalized, &finalized), (HeadKind::Best, &best)] {
				let judge =
					Judge { facts, view: &view, keys: &keys, reads, now, finalized: heads.finalized.number, windows: &windows };
				let guard = token_guard.get(&facts.record).copied().unwrap_or(true);
				let finalized = matches!(head, HeadKind::Finalized);
				proven.insert((facts.record, finalized), (judge.completed(), guard && judge.not_completed()));
			}
		}
		Ok(Box::new(Proven(proven)))
	}
}

/// The answers of one pass, per record and head (true for the finalized
/// head): completed, not completed.
struct Proven(HashMap<(DurableTxId, bool), (bool, bool)>);

impl PassScope for Proven {
	fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
		self.0.get(&(tx.id, matches!(head, HeadKind::Finalized))).is_some_and(|(completed, _)| *completed)
	}

	fn proven_not_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
		self.0.get(&(tx.id, matches!(head, HeadKind::Finalized))).is_some_and(|(_, not_completed)| *not_completed)
	}
}

/// The window of each record that finally minted an input of `records`. A
/// window that cannot be read is left out, so its input is not settled.
async fn minter_windows(store: &Db, records: &[&RecordFacts], view: &Snapshot) -> HashMap<DurableTxId, Window> {
	let minters: HashSet<DurableTxId> = records
		.iter()
		.flat_map(|facts| &facts.rows.inputs)
		.filter_map(|input| view.registrations().creator(input))
		.filter(|minter| matches!(minter.state.status, DurableTxStatus::FinalizedSuccess { .. }))
		.map(|minter| minter.record)
		.collect();
	let read = store.read(move |conn| {
		Ok(minters
			.into_iter()
			.filter_map(|minter| Some((minter, durable::entry(conn, minter).ok()??.window)))
			.collect::<HashMap<_, _>>())
	});
	read.await.unwrap_or_default()
}

/// The account of every coin and the keys of every entry the pass's records
/// name.
#[derive(Default)]
struct AssetKeys {
	accounts: HashMap<Asset, [u8; 32]>,
	entries: HashMap<Asset, EntryKeys>,
}

/// The keys the chain knows an entry by, and its exponent as the chain
/// stores it.
struct EntryKeys {
	member: [u8; 32],
	alias: [u8; 32],
	exponent: i8,
}

impl AssetKeys {
	fn derive(records: &[&RecordFacts], view: &Snapshot, entropy: &[u8], vrf: &Vrf) -> Self {
		let mut keys = Self::default();
		let assets: HashSet<Asset> =
			records.iter().flat_map(|facts| facts.rows.inputs.iter().chain(&facts.rows.outputs)).copied().collect();
		for asset in assets {
			match asset {
				Asset::Coin { purse, installation, index } => {
					if let Ok((_, account)) = coin_key(entropy, purse, installation, index) {
						keys.accounts.insert(asset, account);
					}
				},
				Asset::PeerCoin(account) => {
					keys.accounts.insert(asset, account);
				},
				Asset::Entry { purse, installation, index } => {
					let seed = entry_seed(entropy, purse, installation, index);
					let derived = (
						vrf.member(&seed.0),
						vrf.alias(&seed.0, &assembly::UNLOADING_RECYCLER_CONTEXT),
						view.asset(&asset).and_then(|record| i8::try_from(record.exponent.get()).ok()),
					);
					if let (Ok(member), Ok(alias), Some(exponent)) = derived {
						keys.entries.insert(asset, EntryKeys { member, alias, exponent });
					}
				},
			}
		}
		keys
	}
}

/// What the chain showed at one head; none where a read failed.
struct Reads {
	coins: Option<HashMap<[u8; 32], Option<ChainCoin>>>,
	/// For each member key, the exponent it was loaded under.
	members: Option<HashMap<[u8; 32], Option<i8>>>,
	/// Whether each entry's alias is unloaded, for the entries whose
	/// position and alias state were read: an onboarding entry was never in a
	/// ring, so it was never unloaded. A suspended entry, and one whose read
	/// failed, is absent.
	unloaded: HashMap<Asset, bool>,
}

impl Reads {
	async fn at(chain: &dyn CoinageChain, at: H256, keys: &AssetKeys) -> Self {
		let accounts: Vec<[u8; 32]> = keys.accounts.values().copied().collect();
		let members: Vec<[u8; 32]> = keys.entries.values().map(|entry| entry.member).collect();
		let coins = chain.coins_at(at, &accounts).await.ok().map(|coins| accounts.iter().copied().zip(coins).collect());
		let member_reads = chain.members_at(at, &members).await.ok();
		let members_map = member_reads
			.map(|found| members.iter().copied().zip(found.into_iter().map(|loaded| loaded.map(|(_, exponent)| exponent))).collect());
		Self { coins, members: members_map, unloaded: unloaded_at(chain, at, keys).await }
	}
}

/// Whether each entry's alias is unloaded at `at`, for the entries the chain
/// answers for (see [`Reads`]).
async fn unloaded_at(chain: &dyn CoinageChain, at: H256, keys: &AssetKeys) -> HashMap<Asset, bool> {
	let mut by_exponent: HashMap<i8, Vec<(&Asset, &EntryKeys)>> = HashMap::new();
	for (entry, entry_keys) in &keys.entries {
		by_exponent.entry(entry_keys.exponent).or_default().push((entry, entry_keys));
	}
	let mut unloaded = HashMap::new();
	let mut rings: HashMap<(i8, u32), Vec<(&Asset, &EntryKeys)>> = HashMap::new();
	for (exponent, group) in by_exponent {
		let members: Vec<[u8; 32]> = group.iter().map(|(_, entry_keys)| entry_keys.member).collect();
		let Ok(positions) = chain.places_at(at, exponent, &members).await else { continue };
		for ((entry, entry_keys), position) in group.into_iter().zip(positions) {
			match position {
				Some(RingPosition::Onboarding) => {
					unloaded.insert(*entry, false);
				},
				Some(RingPosition::Included(place)) => rings.entry((exponent, place.ring)).or_default().push((entry, entry_keys)),
				Some(RingPosition::Suspended) | None => {},
			}
		}
	}
	for ((exponent, ring), group) in rings {
		let aliases: Vec<[u8; 32]> = group.iter().map(|(_, entry_keys)| entry_keys.alias).collect();
		if let Ok(states) = chain.aliases_at(at, exponent, ring, &aliases).await {
			let read = states.into_iter().map(|state| state == Some(AliasState::Unloaded));
			unloaded.extend(group.into_iter().map(|(entry, _)| *entry).zip(read));
		}
	}
	unloaded
}

/// The oracle's rules for one record at one head.
struct Judge<'a> {
	facts: &'a RecordFacts,
	view: &'a Snapshot,
	keys: &'a AssetKeys,
	reads: &'a Reads,
	now: u64,
	/// The finalized head's number.
	finalized: u64,
	/// The window of each record that finally minted an input.
	windows: &'a HashMap<DurableTxId, Window>,
}

impl Judge<'_> {
	/// The coin at `asset`'s account: some(some) present with the expected
	/// exponent, some(none) absent, none when unknown.
	fn coin(&self, asset: &Asset) -> Option<Option<ChainCoin>> {
		let account = self.keys.accounts.get(asset)?;
		let found = *self.reads.coins.as_ref()?.get(account)?;
		match found {
			Some(coin) if Some(coin.value) == self.expected(asset) => Some(Some(coin)),
			Some(_) => None,
			None => Some(None),
		}
	}

	fn expected(&self, asset: &Asset) -> Option<i8> {
		i8::try_from(self.view.asset(asset)?.exponent.get()).ok()
	}

	fn member_present(&self, entry: &Asset) -> Option<bool> {
		let member = self.keys.entries.get(entry)?.member;
		let found = *self.reads.members.as_ref()?.get(&member)?;
		Some(found.is_some_and(|exponent| Some(exponent) == self.expected(entry)))
	}

	/// Whether `entry`'s alias is unloaded; none when unknown.
	fn alias_unloaded(&self, entry: &Asset) -> Option<bool> {
		self.reads.unloaded.get(entry).copied()
	}

	fn completed(&self) -> bool {
		let rows = &self.facts.rows;
		let output_present = rows.outputs.iter().any(|output| match output {
			Asset::Entry { .. } => self.member_present(output) == Some(true),
			_ => matches!(self.coin(output), Some(Some(_))),
		});
		let alias_unloaded = rows.inputs.iter().any(|input| self.alias_unloaded(input) == Some(true));
		let settled_and_gone = !rows.inputs.is_empty() &&
			rows.inputs.iter().all(|input| self.settled(input) && matches!(self.coin(input), Some(None)));
		let consumer_final = rows.outputs.iter().any(|output| {
			self.view.registrations().consumers(output).any(|consumer| {
				matches!(consumer.state.status, DurableTxStatus::FinalizedSuccess { .. })
			})
		});
		output_present || alias_unloaded || settled_and_gone || consumer_final
	}

	fn not_completed(&self) -> bool {
		let rows = &self.facts.rows;
		let no_failed_dispatch = match rows.inputs.first() {
			Some(Asset::Coin { .. } | Asset::PeerCoin(_)) => rows.inputs.iter().all(|input| {
				matches!(self.coin(input), Some(coin) if coin.is_none_or(|coin| coin.locked_until.is_none_or(|until| until <= self.now)))
			}),
			_ => true,
		};
		if !no_failed_dispatch {
			return false;
		}
		let available = rows.inputs.iter().any(|input| {
			let own = !matches!(input, Asset::PeerCoin(_)) && self.view.mark(input).is_none();
			own && match input {
				Asset::Entry { .. } => self.member_present(input) == Some(true) && self.alias_unloaded(input) == Some(false),
				_ => matches!(self.coin(input), Some(Some(_))),
			}
		});
		let untouched_absent = rows.outputs.iter().any(|output| {
			matches!(output, Asset::Coin { .. }) &&
				self.view.mark(output).is_none() &&
				!self.view.registrations().consumers(output).any(|consumer| !failed(consumer)) &&
				matches!(self.coin(output), Some(None))
		});
		let dependency_failed = self
			.facts
			.depends_on
			.iter()
			.any(|record| self.view.registrations().record(*record).is_some_and(failed));
		available || untouched_absent || dependency_failed
	}

	/// A coin this layer minted, never disclosed, whose minter succeeded
	/// finally and whose minter's window is closed at the finalized head: one
	/// block past its death, as the durability layer closes a window.
	fn settled(&self, input: &Asset) -> bool {
		matches!(input, Asset::Coin { .. }) &&
			self.view.mark(input).is_none() &&
			self.view.registrations().creator(input).is_some_and(|minter| {
				matches!(minter.state.status, DurableTxStatus::FinalizedSuccess { .. }) &&
					self.windows.get(&minter.record).is_some_and(|window| self.finalized > window.death())
			})
	}
}

fn failed(facts: &RecordFacts) -> bool {
	matches!(facts.state.status, DurableTxStatus::Failure(_))
}

/// For each token-paid unload, whether no failed dispatch shows: its token
/// is not recorded as consumed, in a period still accepted. A record whose
/// token cannot be checked gets false, so the oracle claims nothing.
async fn token_guards(
	deps: &CoinageDeps,
	records: &[&RecordFacts],
	_view: &Snapshot,
	now: u64,
	heads: &Heads,
) -> HashMap<DurableTxId, bool> {
	let mut guards = HashMap::new();
	let presenting: Vec<&&RecordFacts> = records.iter().filter(|facts| !facts.rows.tokens.is_empty()).collect();
	if presenting.is_empty() {
		return guards;
	}
	let running = deps.running.state();
	let person = deps.personhood.person().await;
	for facts in presenting {
		let guard = async {
			let running = running.as_ref()?;
			let (signer, ring, _) = person.as_ref()?;
			let token = *facts.rows.tokens.first()?;
			let grace = running.params.config().token_grace.as_secs();
			let accepted = token.period >= period_of(now.saturating_sub(grace), &running.params);
			let context = assembly::free_token_context(token);
			let alias = signer.alias(ring.collection, &context).ok()?;
			let consumed = deps.people.tokens_consumed_at(heads.finalized.hash, &[(token.period, alias)]).await.ok()?;
			Some(accepted && !consumed.first().copied().unwrap_or(true))
		};
		guards.insert(facts.record, guard.await.unwrap_or(false));
	}
	guards
}

#[cfg(test)]
mod tests;
