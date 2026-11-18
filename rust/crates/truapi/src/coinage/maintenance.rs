//! F2 · Maintenance: everything due at an instant, started as the layer's
//! own operations.
//!
//! Two sweeps move value between its two forms: the rescue unloads entries
//! whose ring is about to expire, which is the only way an intact wallet
//! loses value (invariant 1), and the recycling turns coins into entries.
//! What one call starts depends only on the records (invariant 3), so the
//! entry point is safe to call at any frequency, and it never waits
//! (invariant 4).

use core::time::Duration;
use std::collections::{BTreeMap, BTreeSet};

use super::{
	Asset, Exponent, PurseId,
	classification::{Arriving, CoinClass, Class, EntryLocal, Selectability, classify, recycle_due},
	denominations::denominations,
	ledger::{self, Observed, Read, Snapshot, Subject},
	offload,
	operation::{OpKey, Retry},
	planning::{Destination, PlanInput},
	selection::Selection,
	start::{Launcher, Request, StartError, start},
};
use crate::{
	durable::DurableTxStatus,
	store::{Db, DbError},
};

/// What one call of the entry point did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Due {
	/// The operations it started.
	pub started: Vec<OpKey>,
	/// The purses for which it cannot tell what is due: an asset never
	/// observed, or observed with reads failing since, or a due rescue that
	/// could not start (invariant 5).
	pub cannot_tell: Vec<PurseId>,
	/// How long the host may wait before calling again; zero while
	/// `cannot_tell` is not empty.
	pub call_again_within: Option<Duration>,
}

/// Runs everything due at `now`.
pub async fn run_due(launcher: &Launcher<'_>, store: &Db, now: u64) -> Result<Due, DbError> {
	let view = store.read(ledger::snapshot).await?;
	let params = launcher.params;
	let mut due = Due::default();
	let mut cannot_tell: BTreeSet<PurseId> = unobserved_purses(&view).into_iter().collect();

	for (purse, group, expiry) in rescues(&view, now, params) {
		let value: u64 = group.iter().filter_map(|entry| view.asset(entry)).map(|record| record.exponent.value()).sum();
		let selection =
			Selection { purse, inputs: group, splits: Vec::new(), outputs: denominations(value, params), depends_on: Vec::new() };
		let input = PlanInput::Spend { selection, to: Destination::Purse(purse) };
		match own_operation(launcher, store, Retry::Until(expiry), input, now).await? {
			Some(key) => due.started.push(key),
			None => {
				cannot_tell.insert(purse);
			},
		}
	}

	for (purse, coin) in recyclings(&view, now, params) {
		let input = PlanInput::Recycle { coins: vec![coin], into: purse };
		let until = now + params.token_period().as_secs();
		if let Some(key) = own_operation(launcher, store, Retry::Until(until), input, now).await? {
			due.started.push(key);
		}
	}

	offload::resume_due(launcher, store, now).await?;

	let settled = settled_imports(&view);
	if !settled.is_empty() {
		store.write(move |tx| ledger::forget_secrets(tx, &settled)).await?;
	}

	due.cannot_tell = cannot_tell.into_iter().collect();
	due.call_again_within = if due.cannot_tell.is_empty() { next_deadline(&view, now, params) } else { Some(Duration::ZERO) };
	Ok(due)
}

/// When the default reconstruction becomes due (F4 invariant 5), in Unix
/// seconds; none when it is not needed. It is needed while the registry has
/// not been read since the layer started (`registry_read`), since only a
/// read tells whether it lists an installation not yet scanned, and while a
/// known installation's first scan has not completed. It is due one longest
/// validity window after the current installation was minted, so that every
/// transaction from before a wipe has landed or died first.
pub fn default_reconstruction_at(view: &Snapshot, params: &super::ChainParams, registry_read: bool) -> Option<u64> {
	let unscanned = view.installations().iter().any(|(_, progress)| !progress.first_scan_done);
	if registry_read && !unscanned {
		return None;
	}
	let config = params.config();
	let window = config.block_time.as_secs() * u64::from(config.longest_window);
	Some(view.minted_at().unwrap_or(0) + window)
}

/// Starts one of the layer's own operations; none when its start was refused,
/// for want of tokens or because its inputs were taken first.
async fn own_operation(
	launcher: &Launcher<'_>,
	store: &Db,
	retry: Retry,
	input: PlanInput,
	now: u64,
) -> Result<Option<OpKey>, DbError> {
	let key = OpKey::own(store.write(ledger::next_operation_id).await?);
	match start(launcher, &key, retry, Request::Given(input), now).await {
		Ok(_) => Ok(Some(key)),
		Err(StartError::Db(error)) => Err(error),
		Err(_) => Ok(None),
	}
}

/// The unload groups due for rescue, ordered by ring deadline: the entries,
/// locally available, whose ring is within the rescue margin of its expiry,
/// grouped by purse, exponent and ring revision and cut at the unload cap.
/// An entry under an earlier installation is rescued into the main purse.
fn rescues(view: &Snapshot, now: u64, params: &super::ChainParams) -> Vec<(PurseId, Vec<Asset>, u64)> {
	let mut groups: BTreeMap<(u64, PurseId, Exponent, u32, u32), Vec<Asset>> = BTreeMap::new();
	for record in view.all_assets() {
		let state = classify(&record.asset, view, now, params);
		let rescuable = matches!(state.class, Class::Entry { local: EntryLocal::Available, .. }) &&
			state.selectability == Selectability::Arriving(Arriving::RescueMargin);
		if !rescuable {
			continue;
		}
		let Some(Read::Entry(Some(entry))) = observed_value(view, &record.asset) else { continue };
		let Some(expiry) = entry.expires_at else { continue };
		let purse = owning_purse(view, &record.asset);
		groups.entry((expiry, purse, record.exponent, entry.ring, entry.revision)).or_default().push(record.asset);
	}
	let cap = params.max_unload_entries().max(1) as usize;
	groups
		.into_iter()
		.flat_map(|((expiry, purse, ..), entries)| {
			entries.chunks(cap).map(|chunk| (purse, chunk.to_vec(), expiry)).collect::<Vec<_>>()
		})
		.collect()
}

/// The coins to recycle, oldest first: those `recycle_due` admits, and every
/// available coin at the age cutoff, which is exempt from the cap and the
/// reserve (invariant 6).
fn recyclings(view: &Snapshot, now: u64, params: &super::ChainParams) -> Vec<(PurseId, Asset)> {
	let mut coins: BTreeMap<(core::cmp::Reverse<u32>, Asset), PurseId> = BTreeMap::new();
	let purses: Vec<PurseId> = view.purses().map(|purse| purse.id).collect();
	for purse in purses {
		for coin in recycle_due(view, purse, now, params) {
			coins.insert((core::cmp::Reverse(age(view, &coin)), coin), purse);
		}
		for record in view.assets(purse) {
			let state = classify(&record.asset, view, now, params);
			let at_cutoff = state.class == Class::Coin(CoinClass::Available) &&
				state.selectability == Selectability::Arriving(Arriving::AgeCutoff);
			if at_cutoff {
				coins.insert((core::cmp::Reverse(age(view, &record.asset)), record.asset), purse);
			}
		}
	}
	coins.into_iter().map(|((_, coin), purse)| (purse, coin)).collect()
}

/// The purse an asset's sweep output lands in: its own under the current
/// installation, the main purse under an earlier one (B2 invariant 5).
fn owning_purse(view: &Snapshot, asset: &Asset) -> PurseId {
	match (asset.purse(), asset.installation()) {
		(Some(purse), Some(installation)) if Some(installation) == view.current_installation() => view.resolve_purse(purse),
		_ => PurseId::MAIN,
	}
}

fn observed_value<'a>(view: &'a Snapshot, asset: &Asset) -> Option<&'a Read> {
	view.observation(&Subject::Asset(*asset))?.value.as_ref()
}

fn age(view: &Snapshot, coin: &Asset) -> u32 {
	match observed_value(view, coin) {
		Some(Read::Coin { now: Some(coin), .. }) => coin.age,
		_ => 0,
	}
}

/// The purses with an asset whose observation is missing or whose reads
/// have failed since.
fn unobserved_purses(view: &Snapshot) -> Vec<PurseId> {
	let mut purses = BTreeSet::new();
	for record in view.all_assets() {
		let observed = view.observation(&Subject::Asset(record.asset)).map_or(Observed::Never, |o| o.observed);
		let minting = view.registrations().creator(&record.asset).is_some_and(|facts| {
			!matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. } | DurableTxStatus::Failure(_))
		});
		if !matches!(observed, Observed::At(_)) && !minting {
			purses.insert(owning_purse(view, &record.asset));
		}
	}
	purses.into_iter().collect()
}

/// The peer coins whose claim succeeded finally or whose import ended.
fn settled_imports(view: &Snapshot) -> Vec<[u8; 32]> {
	view.registrations()
		.records()
		.filter(|facts| matches!(facts.state.status, DurableTxStatus::FinalizedSuccess { .. } | DurableTxStatus::Failure(_)))
		.flat_map(|facts| facts.rows.inputs.iter())
		.filter_map(|input| match input {
			Asset::PeerCoin(account) => Some(*account),
			_ => None,
		})
		.collect()
}

/// The time until the next ring reaches its rescue margin.
fn next_deadline(view: &Snapshot, now: u64, params: &super::ChainParams) -> Option<Duration> {
	let margin = params.config().rescue_margin.as_secs();
	view.all_assets()
		.filter_map(|record| match observed_value(view, &record.asset) {
			Some(Read::Entry(Some(entry))) => entry.expires_at.map(|expiry| expiry.saturating_sub(margin)),
			_ => None,
		})
		.filter(|rescue_at| *rescue_at > now)
		.min()
		.map(|rescue_at| Duration::from_secs(rescue_at - now))
}

#[cfg(test)]
mod tests;
