//! F1 · Observation: B2's record of what the chain shows, kept current at
//! each new finalized block.
//!
//! F1 stores facts, never a verdict (invariant 4): an empty account is
//! evidence, and only the durability layer concludes from it (invariant 2).
//! Every value it hands B2 is read at one block (invariant 5), and a read
//! that failed reaches B2 as failed, never as absent.

use std::{collections::HashMap, sync::Arc};

use super::{
	Asset, ChainParams, Exponent, Snapshot,
	assembly::{UNLOADING_RECYCLER_CONTEXT, free_token_context},
	chain::{AliasState, ChainCoin, CoinageChain, RingPosition},
	ledger::{self, BlockRef, CoinOnChain, Emptied, EntryOnChain, Facts, Observed, Read, Subject, TokenId},
	namespace::{coin_key, entry_seed},
	plugins::{PersonhoodSource, Seed},
	tokens::period_of,
};
use crate::{
	chain::HashAndNumber,
	durable::DurableTxStatus,
	runtime::{load_vrf, statement_allowance::collection::PersonhoodCollection},
	store::Db,
};

/// What observation reads and writes.
pub struct ObserverDeps {
	/// The shared store.
	pub store: Db,
	/// The recovery phrase's entropy, which names the accounts and keys read.
	pub seed: Arc<dyn Seed>,
	/// Reads of the People chain.
	pub chain: Arc<dyn CoinageChain>,
	/// The person whose free tokens the layer spends.
	pub personhood: Arc<dyn PersonhoodSource>,
}

/// Why a block was not observed.
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum ObserveError {
	/// The seed or the ring-VRF operations are unavailable.
	#[display("the keys are unavailable")]
	Keys,
	/// The chain's heads could not be read.
	#[display("the chain's heads could not be read")]
	Heads,
	/// The store failed.
	Store(crate::store::DbError),
}

/// Reads, at `block`, the facts of every coin, entry and current-period
/// token identity the ledger holds, and stores them in one store
/// transaction.
pub async fn on_block(deps: &ObserverDeps, block: HashAndNumber, now: u64, params: &ChainParams) -> Result<(), ObserveError> {
	let view = deps.store.read(ledger::snapshot).await?;
	let entropy = deps.seed.entropy().ok_or(ObserveError::Keys)?;
	let vrf = load_vrf().await.map_err(|_| ObserveError::Keys)?;
	let at = block.hash;
	let mut reads: Vec<(Subject, Read)> = Vec::new();

	let mut coins: Vec<(Asset, [u8; 32])> = view
		.all_assets()
		.filter_map(|record| match record.asset {
			Asset::Coin { purse, installation, index } => {
				coin_key(&entropy, purse, installation, index).ok().map(|(_, account)| (record.asset, account))
			},
			_ => None,
		})
		.collect();
	coins.extend(view.registrations().records().flat_map(|facts| {
		facts.rows.inputs.iter().filter_map(|input| match input {
			Asset::PeerCoin(account) => Some((*input, *account)),
			_ => None,
		})
	}));
	let accounts: Vec<[u8; 32]> = coins.iter().map(|(_, account)| *account).collect();
	match deps.chain.coins_at(at, &accounts).await {
		Ok(found) => {
			let finalized = deps.chain.heads().await.ok().map(|heads| heads.finalized.number);
			for ((asset, account), coin) in coins.iter().zip(found) {
				let now = coin_on_chain(coin, params);
				let emptied_by = match now {
					None => emptied_by(deps, &view, asset, *account, block, finalized).await,
					Some(_) => None,
				};
				reads.push((Subject::Asset(*asset), Read::Coin { now, emptied_by }));
			}
		},
		Err(_) => reads.extend(coins.iter().map(|(asset, _)| (Subject::Asset(*asset), Read::Failed))),
	}

	let mut entries: HashMap<i8, Vec<EntryKeys>> = HashMap::new();
	for record in view.all_assets() {
		if let Asset::Entry { purse, installation, index } = record.asset {
			let seed = entry_seed(&entropy, purse, installation, index);
			let (Ok(member), Ok(alias)) = (vrf.member(&seed.0), vrf.alias(&seed.0, &UNLOADING_RECYCLER_CONTEXT)) else {
				continue;
			};
			let exponent = i8::try_from(record.exponent.get()).unwrap_or(i8::MAX);
			entries.entry(exponent).or_default().push(EntryKeys { asset: record.asset, member, alias });
		}
	}
	for (exponent, group) in entries {
		let members: Vec<[u8; 32]> = group.iter().map(|keys| keys.member).collect();
		let Ok(places) = deps.chain.places_at(at, exponent, &members).await else {
			reads.extend(group.iter().map(|keys| (Subject::Asset(keys.asset), Read::Failed)));
			continue;
		};
		for (EntryKeys { asset, alias, .. }, place) in group.iter().zip(places) {
			let read = match place {
				None | Some(RingPosition::Onboarding | RingPosition::Suspended) => Read::Entry(None),
				Some(RingPosition::Included(place)) => match deps.chain.aliases_at(at, exponent, place.ring, &[*alias]).await {
					Err(_) => Read::Failed,
					Ok(states) => {
						let lifetime = params.config().ring_lifetime.as_secs();
						let unloaded = deps.chain.unloaded_at(at, exponent, place.ring).await.ok().flatten();
						Read::Entry(Some(EntryOnChain {
							ring: place.ring,
							revision: place.revision,
							page: place.page,
							position: place.position,
							members: place.members,
							included: place.included,
							closed: place.immutable_since.is_some(),
							expires_at: place.immutable_since.map(|since| since + lifetime),
							locked_until: match states.first() {
								Some(Some(AliasState::Locked(until))) => Some(*until),
								_ => None,
							},
							unloaded,
						}))
					},
				},
			};
			reads.push((Subject::Asset(*asset), read));
		}
	}

	if let Some((signer, ring, _)) = deps.personhood.person().await {
		let allowance = match deps.chain.free_token_limits_at(block).await {
			Ok(limits) => {
				let allowance = match ring.collection {
					PersonhoodCollection::People => limits.people,
					PersonhoodCollection::LitePeople => limits.lite_people,
				};
				reads.push((Subject::UnloadAllowance, Read::UnloadAllowance(allowance)));
				allowance
			},
			Err(_) => {
				reads.push((Subject::UnloadAllowance, Read::Failed));
				view.unload_allowance().unwrap_or(0)
			},
		};
		let period = period_of(now, params);
		let tokens: Vec<(TokenId, [u8; 32])> = (0..allowance)
			.filter_map(|slot| {
				let token = TokenId { period, slot };
				signer.alias(ring.collection, &free_token_context(token)).ok().map(|alias| (token, alias))
			})
			.collect();
		let keys: Vec<(u32, [u8; 32])> = tokens.iter().map(|(token, alias)| (token.period, *alias)).collect();
		match deps.chain.tokens_consumed_at(at, &keys).await {
			Ok(consumed) => reads.extend(
				tokens.iter().zip(consumed).map(|((token, _), consumed)| (Subject::Token(*token), Read::Token { consumed })),
			),
			Err(_) => reads.extend(tokens.iter().map(|(token, _)| (Subject::Token(*token), Read::Failed))),
		}
	}

	store(deps, &view, block, reads, now, params).await
}

/// Reads the peer coins at `accounts` at the finalized head and stores what
/// the chain shows, so that an import plans its outputs at its coins'
/// observed exponents (B1). A failed read stores nothing, and a coin not on
/// chain yet gets its output's denomination when F1 first sees it.
pub async fn observe_peer_coins(
	deps: &ObserverDeps,
	accounts: &[[u8; 32]],
	now: u64,
	params: &ChainParams,
) -> Result<(), ObserveError> {
	let view = deps.store.read(ledger::snapshot).await?;
	let Ok(heads) = deps.chain.heads().await else { return Ok(()) };
	let block = heads.finalized;
	let Ok(found) = deps.chain.coins_at(block.hash, accounts).await else { return Ok(()) };
	let reads = accounts
		.iter()
		.zip(found)
		.map(|(account, coin)| {
			(Subject::Asset(Asset::PeerCoin(*account)), Read::Coin { now: coin_on_chain(coin, params), emptied_by: None })
		})
		.collect();
	store(deps, &view, block, reads, now, params).await
}

/// Stores `reads`, made at `block`, in one store transaction, with the
/// denomination of every import output whose claimed coin they show for
/// the first time.
async fn store(
	deps: &ObserverDeps,
	view: &Snapshot,
	block: HashAndNumber,
	reads: Vec<(Subject, Read)>,
	now: u64,
	params: &ChainParams,
) -> Result<(), ObserveError> {
	let outputs = unfixed_import_outputs(view, &reads);
	let number = u32::try_from(block.number).unwrap_or(u32::MAX);
	let facts = Facts { at: BlockRef { number, hash: block.hash.0 }, reads };
	let page_size = params.config().ring_page_size;
	deps.store
		.write(move |tx| {
			ledger::observe(tx, &facts, now, page_size)?;
			for (output, exponent) in &outputs {
				ledger::fix_import_output(tx, *output, *exponent)?;
			}
			Ok(())
		})
		.await?;
	Ok(())
}

/// The outputs of claims still waiting to be built, of peer coins `reads`
/// shows present, whose denomination is not yet the coin's: the import
/// allocated them before anyone had seen the coin (B1).
fn unfixed_import_outputs(view: &Snapshot, reads: &[(Subject, Read)]) -> Vec<(Asset, Exponent)> {
	let mut outputs = Vec::new();
	for (subject, read) in reads {
		let (Subject::Asset(coin @ Asset::PeerCoin(_)), Read::Coin { now: Some(found), .. }) = (subject, read) else { continue };
		let waiting = view
			.registrations()
			.consumers(coin)
			.filter(|claim| claim.state.status == DurableTxStatus::PendingSubmission);
		for output in waiting.flat_map(|claim| claim.rows.outputs.iter()) {
			let unseen = view.observation(&Subject::Asset(*output)).is_none();
			if unseen && view.asset(output).is_some_and(|record| record.exponent != found.exponent) {
				outputs.push((*output, found.exponent));
			}
		}
	}
	outputs
}

/// A coin as the layer counts it: one of the configured instance at a legal
/// exponent; none for an empty account or any other coin.
fn coin_on_chain(coin: Option<ChainCoin>, params: &ChainParams) -> Option<CoinOnChain> {
	let coin = coin.filter(|coin| coin.instance == params.config().instance)?;
	Some(CoinOnChain {
		exponent: Exponent::from_chain(coin.value, params)?,
		age: u32::from(coin.age),
		locked_until: coin.locked_until,
	})
}

/// For an exported coin whose account is empty at `block`, the transaction
/// that emptied it (B1 `Claimed`), stable once the block that included it
/// is at or below the `finalized` height; none for any other coin, for a
/// coin this layer's own record moves, and while the search finds nothing.
/// A move found stable is not searched for again.
async fn emptied_by(
	deps: &ObserverDeps,
	view: &Snapshot,
	asset: &Asset,
	account: [u8; 32],
	block: HashAndNumber,
	finalized: Option<u64>,
) -> Option<Emptied> {
	let registrations = view.registrations();
	let minted = registrations
		.creator(asset)
		.map(|minter| match minter.state.status {
			DurableTxStatus::FinalizedSuccess { at } => at.map_or(Some(0), |at| Some(at.number)),
			_ => None,
		})
		.unwrap_or(Some(0))?;
	let moved_here = registrations.consumers(asset).any(|consumer| !matches!(consumer.state.status, DurableTxStatus::Failure(_)));
	if view.mark(asset).is_none() || moved_here {
		return None;
	}
	let observation = view.observation(&Subject::Asset(*asset));
	let since = match observation.map(|observation| (observation.observed, observation.value.as_ref())) {
		Some((_, Some(Read::Coin { emptied_by: Some(emptied), .. }))) if emptied.stable => return Some(*emptied),
		Some((Observed::At(at), Some(Read::Coin { now: Some(_), .. }))) => u64::from(at.number),
		_ => minted,
	};
	let emptying = deps.chain.emptied_by(account, since, block).await.ok()??;
	Some(Emptied { by: emptying.transaction, stable: finalized.is_some_and(|finalized| emptying.block <= finalized) })
}

/// An entry and the keys the chain knows it by.
struct EntryKeys {
	asset: Asset,
	member: [u8; 32],
	alias: [u8; 32],
}

/// The same reads at the latest finalized block, for a deployment that
/// cannot subscribe to new blocks (invariant 6): a degraded mode.
pub async fn observe_now(deps: &ObserverDeps, now: u64, params: &ChainParams) -> Result<(), ObserveError> {
	let heads = deps.chain.heads().await.map_err(|_| ObserveError::Heads)?;
	on_block(deps, heads.finalized, now, params).await
}

#[cfg(test)]
mod tests;
