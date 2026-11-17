//! One walk over a [`World`]: the actions the chain and the layer could take
//! next, applying one, and the invariants every reachable state keeps.
//!
//! The chain is the ground truth: every claim the ledger makes is checked
//! against it. `Pending` is always allowed, so this catches a verdict that
//! is confident and wrong, never one that is merely undecided. A failed
//! dispatch changes nothing the layer reads, as on chain the coin is put
//! back and an unload's aliases roll back with the dispatch.

use std::collections::{HashMap, HashSet};

use rand::{Rng, rngs::StdRng};
use subxt::utils::H256;

use crate::{
	chain::DispatchOutcome,
	coinage::{
		Asset,
		chain::RingPosition,
		operation::{OpKind, Retry},
		planning::Call,
		registration::{Admission, RecordFacts, Rows},
		start::RecordParams,
		testing::world::{Finality, World},
	},
	durable::{DurableTxEntry, DurableTxId, DurableTxStatus},
};

/// The coins the wallet starts with, minted by nobody it knows.
const SEED_COINS: [u32; 3] = [1, 2, 3];
/// The entries the wallet starts with, in [`SEED_RING`].
const SEED_ENTRIES: [u32; 2] = [7, 8];
const SEED_RING: u32 = 5;
const SEED_EXPONENT: u8 = 3;
/// Entries a walk may load, each at most once: keys are never reused.
const MINTABLE_ENTRIES: core::ops::RangeInclusive<u32> = 9..=128;
/// More than one of each, so a rule reading the wrong one reads the wrong key.
const EXPONENTS: [u8; 2] = [3, 4];
const RINGS: [u32; 2] = [5, 6];
/// The first coin a walk mints.
const FIRST_OUTPUT_COIN: u32 = 100;
/// Short, so windows open and close as a walk runs.
const PERIOD: u64 = 16;
/// One pass is not a fixpoint: a record a reorg demoted spends that pass
/// being demoted and is decided by the next.
const PASSES_TO_DECIDE: u32 = 2;

/// A read that fails for the length of one pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
	/// Coin reads at both heads.
	Coins,
	/// Every entry's alias.
	Aliases,
	/// Recycler memberships.
	Memberships,
	/// Ring positions.
	Positions,
	/// Every canonical hash, which takes out the search and Rule 0's check.
	Blocks,
	/// Every attempt's dispatch outcome.
	Outcomes,
	/// The heads, so the pass reads nothing.
	Pin,
}

/// Every fault, in a fixed order.
pub const FAULTS: [Fault; 7] =
	[Fault::Coins, Fault::Aliases, Fault::Memberships, Fault::Positions, Fault::Blocks, Fault::Outcomes, Fault::Pin];

/// What a walk can do. Coins and entries are named by their number in the
/// world, records by their identity, so a recorded walk replays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
	/// An empty block on the best chain.
	ProduceBlock,
	/// Finalizes the best chain.
	Finalize,
	/// A pass with every read working.
	RunPass,
	/// A pass with one read failing throughout.
	FaultyPass(Fault),
	/// Drops this many unfinalized blocks.
	Reorg(u64),
	/// A coin into a fresh coin.
	Spend(u32),
	/// A coin into nothing the chain shows.
	Offboard(u32),
	/// A coin into two fresh coins.
	Split(u32),
	/// An entry into a fresh coin.
	Unload(u32),
	/// Two entries into a fresh coin.
	MultiUnload(u32, u32),
	/// A coin into a fresh entry.
	Recycle(u32, u32),
	/// Nothing into a fresh entry, loaded from outside.
	Load(u32),
	/// A coin's key leaves for a peer, its mark committed.
	HandOff(u32),
	/// The peer spends a coin handed to them.
	PeerSpends(u32),
	/// Two spends registered as one operation.
	Batch(u32, u32),
	/// The process dies and starts again without G3.
	Crash,
	/// The process dies and starts again through G3.
	Relaunch,
	/// An entry's ring is archived.
	Archive(u32),
	/// An onboarding entry takes a place in a ring.
	Place(u32, u32),
	/// A record's attempt lands in a block, its dispatch succeeding or not.
	Include(i64, bool),
}

/// Groups of actions a profile weighs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
	/// [`Action::ProduceBlock`].
	ProduceBlock,
	/// [`Action::Finalize`].
	Finalize,
	/// [`Action::Reorg`].
	Reorg,
	/// [`Action::RunPass`].
	RunPass,
	/// [`Action::FaultyPass`].
	FaultyPass,
	/// [`Action::Spend`].
	Spend,
	/// [`Action::Unload`].
	Unload,
	/// [`Action::MultiUnload`].
	MultiUnload,
	/// [`Action::Offboard`].
	Offboard,
	/// [`Action::Split`].
	Split,
	/// [`Action::Recycle`].
	Recycle,
	/// [`Action::Load`].
	Load,
	/// [`Action::Archive`].
	Archive,
	/// [`Action::Place`].
	Place,
	/// [`Action::Include`].
	Include,
	/// [`Action::HandOff`].
	HandOff,
	/// [`Action::PeerSpends`].
	PeerSpends,
	/// [`Action::Batch`].
	Batch,
	/// [`Action::Crash`].
	Crash,
	/// [`Action::Relaunch`].
	Relaunch,
}

impl Action {
	/// The group a profile weighs this action by.
	pub fn kind(&self) -> Kind {
		match self {
			Self::ProduceBlock => Kind::ProduceBlock,
			Self::Finalize => Kind::Finalize,
			Self::RunPass => Kind::RunPass,
			Self::FaultyPass(_) => Kind::FaultyPass,
			Self::Reorg(_) => Kind::Reorg,
			Self::Spend(_) => Kind::Spend,
			Self::Offboard(_) => Kind::Offboard,
			Self::Split(_) => Kind::Split,
			Self::Unload(_) => Kind::Unload,
			Self::MultiUnload(..) => Kind::MultiUnload,
			Self::Recycle(..) => Kind::Recycle,
			Self::Load(_) => Kind::Load,
			Self::HandOff(_) => Kind::HandOff,
			Self::PeerSpends(_) => Kind::PeerSpends,
			Self::Batch(..) => Kind::Batch,
			Self::Crash => Kind::Crash,
			Self::Relaunch => Kind::Relaunch,
			Self::Archive(_) => Kind::Archive,
			Self::Place(..) => Kind::Place,
			Self::Include(..) => Kind::Include,
		}
	}
}

/// How often a walk takes each kind of action: a kind first, by weight, then
/// one of its actions uniformly.
pub struct Profile {
	/// The name a failure reports.
	pub name: &'static str,
	/// Weights other than the ordinary one.
	pub overrides: &'static [(Kind, u32)],
}

impl Profile {
	const ORDINARY: u32 = 10;
	const RARE_FAULTS: u32 = 3;

	fn weight(&self, kind: Kind) -> u32 {
		let default = if kind == Kind::FaultyPass { Self::RARE_FAULTS } else { Self::ORDINARY };
		self.overrides.iter().find(|(overridden, _)| *overridden == kind).map_or(default, |(_, weight)| *weight)
	}

	/// One of `enabled`, or none when every enabled kind weighs nothing.
	pub fn pick(&self, rng: &mut StdRng, enabled: &[Action]) -> Option<Action> {
		let mut kinds: Vec<Kind> = Vec::new();
		for action in enabled {
			if !kinds.contains(&action.kind()) && self.weight(action.kind()) > 0 {
				kinds.push(action.kind());
			}
		}
		let total: u32 = kinds.iter().map(|kind| self.weight(*kind)).sum();
		if total == 0 {
			return None;
		}
		let mut roll = rng.gen_range(0..total);
		let kind = *kinds.iter().find(|kind| {
			let weight = self.weight(**kind);
			let hit = roll < weight;
			roll = roll.saturating_sub(weight);
			hit
		})?;
		let choices: Vec<Action> = enabled.iter().copied().filter(|action| action.kind() == kind).collect();
		Some(choices[rng.gen_range(0..choices.len())])
	}
}

/// An invariant a walk broke.
#[derive(Debug)]
pub struct Violation(pub String);

/// Why a replay stopped.
#[derive(Debug)]
pub enum Stop {
	/// An invariant broke.
	Violated(Violation),
	/// An action was not enabled: the candidate is not a legal history.
	Diverged,
}

/// Where an attempt landed on the canonical chain, and how it dispatched.
#[derive(Clone, Copy)]
struct Inclusion {
	number: u64,
	succeeded: bool,
}

/// One walk's world and what its invariants remember across actions.
pub struct Driver {
	world: World,
	coins: Vec<(u32, Asset)>,
	entries: HashMap<u32, Asset>,
	next_coin: u32,
	expired_passes: HashMap<DurableTxId, u32>,
	settled_passes: HashMap<DurableTxId, u32>,
	committed: HashSet<Asset>,
	terminal: HashMap<DurableTxId, DurableTxStatus>,
}

impl Driver {
	/// A world holding the seed coins and entries.
	pub fn new() -> Self {
		let mut world = World::new();
		world.set_period(PERIOD);
		let coins: Vec<(u32, Asset)> = SEED_COINS.iter().map(|n| (*n, world.coin(*n))).collect();
		let assets: Vec<Asset> = coins.iter().map(|(_, coin)| *coin).collect();
		world.mint(&assets, Finality::Finalized);
		let mut entries = HashMap::new();
		for n in SEED_ENTRIES {
			let entry = world.entry_of(n, SEED_EXPONENT);
			world.place(entry, SEED_RING, Finality::Finalized);
			entries.insert(n, entry);
		}
		Self {
			world,
			coins,
			entries,
			next_coin: FIRST_OUTPUT_COIN,
			expired_passes: HashMap::new(),
			settled_passes: HashMap::new(),
			committed: HashSet::new(),
			terminal: HashMap::new(),
		}
	}

	/// Takes `steps` actions `profile` draws with `rng`, checking the
	/// invariants after each, and returns them.
	pub fn walk(&mut self, rng: &mut StdRng, steps: usize, profile: &Profile) -> Result<Vec<Action>, (Violation, Vec<Action>)> {
		let mut taken = Vec::new();
		for _ in 0..steps {
			let Some(action) = profile.pick(rng, &self.enabled()) else { continue };
			self.apply(action);
			taken.push(action);
			if let Err(violation) = self.check(action) {
				return Err((violation, taken));
			}
		}
		Ok(taken)
	}

	/// Replays `actions` exactly, or stops at the first that is not enabled.
	pub fn replay(&mut self, actions: &[Action]) -> Result<(), Stop> {
		for action in actions {
			if !self.enabled().contains(action) {
				return Err(Stop::Diverged);
			}
			self.apply(*action);
			self.check(*action).map_err(Stop::Violated)?;
		}
		Ok(())
	}

	fn coin(&self, n: u32) -> Asset {
		self.coins.iter().find(|(known, _)| *known == n).expect("a coin of the walk").1
	}

	fn fresh_coin(&mut self) -> Asset {
		let n = self.next_coin;
		self.next_coin += 1;
		let coin = self.world.coin(n);
		self.coins.push((n, coin));
		coin
	}

	fn entry(&mut self, n: u32) -> Asset {
		if let Some(entry) = self.entries.get(&n) {
			return *entry;
		}
		let exponent = EXPONENTS[usize::try_from(n).expect("small") % EXPONENTS.len()];
		let entry = self.world.entry_of(n, exponent);
		self.entries.insert(n, entry);
		entry
	}

	/// Every record of the walk with an attempt, with its attempt; the
	/// installation's registration, which the wallet starts with, has none.
	fn attempts(&self) -> Vec<DurableTxEntry> {
		let view = self.world.view();
		let walked = |facts: &&RecordFacts| view.operation(&facts.operation).is_some_and(|operation| operation.kind != OpKind::RegisterInstallation);
		view.registrations().records().filter(walked).filter_map(|facts| self.world.try_attempt(facts.record)).collect()
	}

	/// The first canonical inclusion of every attempt, up to the best head.
	fn canonical(&self) -> HashMap<H256, Inclusion> {
		let tree = self.world.chain().tree();
		let mut inclusions = HashMap::new();
		for number in 0..=tree.best().number {
			let block = tree.canonical(number).expect("a canonical block");
			for tx in tree.body(block.hash) {
				let succeeded = tree.outcome_in(block.hash, tx) == Some(DispatchOutcome::Succeeded);
				inclusions.entry(tx).or_insert(Inclusion { number, succeeded });
			}
		}
		inclusions
	}

	fn coin_present_at_best(&self, coin: Asset) -> bool {
		self.world.chain().tree().best_state().coins.contains_key(&self.world.account(coin))
	}

	fn member_at_best(&self, entry: Asset) -> Option<RingPosition> {
		let (member, _) = self.world.entry_keys(entry);
		self.world.chain().tree().best_state().members.get(&member).map(|(_, position)| *position)
	}

	/// In a ring at the best head with its alias not unloaded there.
	fn unloadable_at_best(&self, entry: Asset) -> bool {
		let Some(RingPosition::Included(place)) = self.member_at_best(entry) else { return false };
		let (_, alias) = self.world.entry_keys(entry);
		let key = (self.world.exponent_of(entry), place.ring, alias);
		!self.world.chain().tree().best_state().aliases.contains_key(&key)
	}

	/// What the chain and the layer could do next.
	pub fn enabled(&self) -> Vec<Action> {
		let mut enabled = vec![Action::ProduceBlock, Action::Finalize, Action::RunPass];
		enabled.extend(FAULTS.map(Action::FaultyPass));
		let (best, finalized) = {
			let tree = self.world.chain().tree();
			(tree.best().number, tree.finalized().number)
		};
		enabled.extend((1..=best - finalized).map(Action::Reorg));
		let view = self.world.view();
		let live = || view.registrations().records().filter(|facts| !matches!(facts.state.status, DurableTxStatus::Failure(_)));
		let claimed: HashSet<Asset> = live().flat_map(|facts| facts.rows.inputs.iter().copied()).collect();
		let present: Vec<u32> =
			self.coins.iter().filter(|(_, coin)| self.coin_present_at_best(*coin)).map(|(n, _)| *n).collect();
		let unclaimed: Vec<u32> = present.iter().copied().filter(|n| !claimed.contains(&self.coin(*n))).collect();
		for n in &unclaimed {
			enabled.extend([Action::Spend(*n), Action::Offboard(*n), Action::Split(*n)]);
		}
		let mut entries: Vec<(u32, Asset)> = self.entries.iter().map(|(n, entry)| (*n, *entry)).collect();
		entries.sort();
		let unloadable: Vec<u32> = entries
			.iter()
			.filter(|(_, entry)| self.unloadable_at_best(*entry) && !claimed.contains(entry))
			.map(|(n, _)| *n)
			.collect();
		enabled.extend(unloadable.iter().map(|n| Action::Unload(*n)));
		if let [first, second, ..] = unloadable[..] {
			enabled.push(Action::MultiUnload(first, second));
		}
		for (n, entry) in &entries {
			let Some(position) = self.member_at_best(*entry) else { continue };
			enabled.push(Action::Archive(*n));
			if position == RingPosition::Onboarding {
				enabled.extend(RINGS.map(|ring| Action::Place(*n, ring)));
			}
		}
		let marked = |n: &u32| view.mark(&self.coin(*n)).is_some();
		let spendable: Vec<u32> = unclaimed.iter().copied().filter(|n| !marked(n)).collect();
		enabled.extend(spendable.iter().map(|n| Action::HandOff(*n)));
		enabled.extend(present.iter().filter(|n| marked(n)).map(|n| Action::PeerSpends(*n)));
		if let [first, second, ..] = spendable[..] {
			enabled.push(Action::Batch(first, second));
		}
		enabled.extend([Action::Crash, Action::Relaunch]);
		if let Some(free) = MINTABLE_ENTRIES.clone().find(|n| !self.entries.contains_key(n)) {
			enabled.extend(unclaimed.iter().map(|n| Action::Recycle(*n, free)));
			enabled.push(Action::Load(free));
		}
		let canonical = self.canonical();
		for attempt in self.attempts() {
			let rows = &view.registrations().record(attempt.id).expect("a coinage record").rows;
			let within_era = (attempt.window.birth..attempt.window.death()).contains(&(best + 1));
			let inputs_there = rows.inputs.iter().all(|input| match input {
				Asset::Entry { .. } => self.unloadable_at_best(*input),
				_ => self.coin_present_at_best(*input),
			});
			let outputs_absent = rows.outputs.iter().all(|output| match output {
				Asset::Entry { .. } => self.member_at_best(*output).is_none(),
				_ => !self.coin_present_at_best(*output),
			});
			let includable = attempt.status.awaits_verdict() &&
				!canonical.contains_key(&attempt.tx_hash) &&
				within_era && inputs_there && outputs_absent;
			if includable {
				enabled.extend([Action::Include(attempt.id.0, true), Action::Include(attempt.id.0, false)]);
			}
		}
		enabled
	}

	/// Registers `records` as one operation that never retries, each with a
	/// scripted attempt it then lets go of. A refused registration changes
	/// nothing.
	fn register(&mut self, records: Vec<Rows>) {
		let call = match records[0].inputs.first() {
			Some(Asset::Entry { .. }) => Call::UnloadToCoins,
			_ => Call::Transfer,
		};
		let params = RecordParams { call, retry: Retry::Never };
		if let Ok(ids) = self.world.register_with(Admission::Ordinary, records, params, true) {
			self.world.release(&ids);
		}
	}

	/// Applies `action`, which [`Self::enabled`] offered.
	pub fn apply(&mut self, action: Action) {
		let rows = |inputs: Vec<Asset>, outputs: Vec<Asset>| Rows { inputs, outputs, tokens: Vec::new() };
		match action {
			Action::ProduceBlock => self.world.advance(1, Finality::InBest),
			Action::Finalize => self.world.finalize(),
			Action::RunPass => self.world.run_pass(),
			Action::FaultyPass(fault) => self.faulty_pass(fault),
			Action::Reorg(depth) => self.world.reorg(depth),
			Action::Spend(n) => {
				let (input, output) = (self.coin(n), self.fresh_coin());
				self.register(vec![rows(vec![input], vec![output])]);
			},
			Action::Offboard(n) => {
				let input = self.coin(n);
				self.register(vec![rows(vec![input], Vec::new())]);
			},
			Action::Split(n) => {
				let (input, first, second) = (self.coin(n), self.fresh_coin(), self.fresh_coin());
				self.register(vec![rows(vec![input], vec![first, second])]);
			},
			Action::Unload(n) => {
				let (input, output) = (self.entry(n), self.fresh_coin());
				self.register(vec![rows(vec![input], vec![output])]);
			},
			Action::MultiUnload(first, second) => {
				let (first, second, output) = (self.entry(first), self.entry(second), self.fresh_coin());
				self.register(vec![rows(vec![first, second], vec![output])]);
			},
			Action::Recycle(n, entry) => {
				let (input, output) = (self.coin(n), self.entry(entry));
				self.register(vec![rows(vec![input], vec![output])]);
			},
			Action::Load(entry) => {
				let output = self.entry(entry);
				self.register(vec![rows(Vec::new(), vec![output])]);
			},
			Action::HandOff(n) => {
				let coin = self.coin(n);
				if self.world.disclose(coin, true).is_ok() {
					self.committed.insert(coin);
				}
			},
			Action::PeerSpends(n) => {
				self.world.consume(self.coin(n), Finality::InBest);
			},
			Action::Batch(first, second) => {
				let (first, first_output) = (self.coin(first), self.fresh_coin());
				let (second, second_output) = (self.coin(second), self.fresh_coin());
				self.register(vec![rows(vec![first], vec![first_output]), rows(vec![second], vec![second_output])]);
			},
			Action::Crash => self.world.crash(),
			Action::Relaunch => self.world.relaunch(),
			Action::Archive(n) => self.world.archive(self.entries[&n], Finality::InBest),
			Action::Place(n, ring) => self.world.place(self.entries[&n], ring, Finality::InBest),
			Action::Include(id, true) => {
				self.world.execute(DurableTxId(id), Finality::InBest);
			},
			Action::Include(id, false) => {
				self.world.include(DurableTxId(id), DispatchOutcome::Failed("Coinage.Locked".into()), Finality::InBest);
			},
		}
	}

	/// A pass during which `fault`'s reads fail. A batched read has no
	/// per-key error, so the whole read fails.
	fn faulty_pass(&mut self, fault: Fault) {
		let aliases: Vec<[u8; 32]> = self.entries.values().map(|entry| self.world.entry_keys(*entry).1).collect();
		let attempts: Vec<H256> = self.attempts().iter().map(|attempt| attempt.tx_hash).collect();
		{
			let mut tree = self.world.chain().tree();
			let heads = [tree.finalized().hash, tree.best().hash];
			let faults = &mut tree.faults;
			match fault {
				Fault::Coins => faults.coins_at.extend(heads),
				Fault::Aliases => faults.aliases.extend(aliases),
				Fault::Memberships => faults.memberships = true,
				Fault::Positions => faults.positions = true,
				Fault::Blocks => faults.every_height = true,
				Fault::Outcomes => faults.outcomes.extend(attempts),
				Fault::Pin => faults.heads = true,
			}
		}
		self.world.run_pass();
		self.world.chain().tree().faults = Default::default();
	}

	/// Checks every invariant after `last`, the action just applied.
	pub fn check(&mut self, last: Action) -> Result<(), Violation> {
		let canonical = self.canonical();
		let (finalized, finalized_coins) = {
			let tree = self.world.chain().tree();
			let state = tree.finalized_state().coins.keys().copied().collect::<HashSet<_>>();
			(tree.finalized().number, state)
		};
		let clean_pass = last == Action::RunPass;
		let view = self.world.view();
		for attempt in self.attempts() {
			let id = attempt.id;
			let status = attempt.status;
			let inclusion = canonical.get(&attempt.tx_hash).copied();
			let succeeded_below = inclusion.is_some_and(|inclusion| inclusion.succeeded && inclusion.number <= finalized);
			let failed_below = inclusion.is_some_and(|inclusion| !inclusion.succeeded && inclusion.number <= finalized);
			let closed = finalized > attempt.window.death();
			if let Some(frozen) = self.terminal.get(&id) {
				ensure(*frozen == status, || format!("record {} changed from terminal {frozen:?} to {status:?}", id.0))?;
			}
			if status.is_final() {
				self.terminal.insert(id, status);
			}
			if let DurableTxStatus::Failure(kind) = status {
				ensure(failed_below || (closed && !succeeded_below), || {
					format!("record {} failed as {kind:?} with nothing final to justify it", id.0)
				})?;
				let rows = &view.registrations().record(id).expect("a coinage record").rows;
				let minted = rows.outputs.iter().filter(|output| !matches!(output, Asset::Entry { .. }));
				let present = minted.filter(|output| finalized_coins.contains(&self.world.account(**output))).count();
				ensure(present == 0, || format!("record {} failed but the finalized chain holds {present} of its outputs", id.0))?;
			}
			if matches!(status, DurableTxStatus::FinalizedSuccess { .. }) {
				ensure(succeeded_below, || {
					format!("record {} succeeded finally with no successful canonical dispatch at or below the finalized head", id.0)
				})?;
			}
			if !clean_pass {
				continue;
			}
			if let DurableTxStatus::PendingSuccess { at } = status {
				let canonical_hash = self.world.chain().tree().canonical(at.number).map(|block| block.hash);
				ensure(canonical_hash == Some(at.hash), || {
					format!("record {} kept a record of block {} across a pass, which is not canonical", id.0, at.number)
				})?;
			}
			if closed {
				let passes = *self.expired_passes.entry(id).and_modify(|passes| *passes += 1).or_insert(1);
				ensure(status != DurableTxStatus::Pending || passes < PASSES_TO_DECIDE, || {
					format!("record {} is still pending after {passes} clean passes past its window", id.0)
				})?;
			}
			if succeeded_below {
				let passes = *self.settled_passes.entry(id).and_modify(|passes| *passes += 1).or_insert(1);
				let final_success = matches!(status, DurableTxStatus::FinalizedSuccess { .. });
				ensure(final_success || passes < PASSES_TO_DECIDE, || {
					format!("record {} is {status:?} after {passes} clean passes, though it succeeded below the finalized head", id.0)
				})?;
			}
		}
		let live: Vec<_> =
			view.registrations().records().filter(|facts| !matches!(facts.state.status, DurableTxStatus::Failure(_))).collect();
		let marked_input = live.iter().find(|facts| facts.rows.inputs.iter().any(|input| view.mark(input).is_some()));
		ensure(marked_input.is_none(), || format!("record {:?} holds a disclosed input", marked_input.map(|facts| facts.record)))?;
		let lost: Vec<&Asset> = self.committed.iter().filter(|asset| view.mark(asset).is_none()).collect();
		ensure(lost.is_empty(), || format!("committed marks did not survive: {lost:?}"))?;
		let mut claimants: HashMap<Asset, usize> = HashMap::new();
		for input in live.iter().flat_map(|facts| &facts.rows.inputs) {
			*claimants.entry(*input).or_default() += 1;
		}
		let shared = claimants.iter().find(|(_, count)| **count > 1);
		ensure(shared.is_none(), || format!("an input is held by more than one record that has not failed: {shared:?}"))
	}
}

fn ensure(holds: bool, message: impl FnOnce() -> String) -> Result<(), Violation> {
	if holds { Ok(()) } else { Err(Violation(message())) }
}
