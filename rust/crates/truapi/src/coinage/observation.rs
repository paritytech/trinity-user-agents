//! F1 · Observation: B2's record of what the chain shows, kept current at
//! each new finalized block.
//!
//! F1 stores facts, never a verdict (invariant 4): an empty account is
//! evidence, and only the durability layer concludes from it (invariant 2).
//! Every value it hands B2 is read at one block (invariant 5), and a read
//! that failed reaches B2 as failed, never as absent.

use std::{collections::HashMap, sync::Arc};

use super::{
	Asset, ChainParams, Exponent,
	assembly::{UNLOADING_RECYCLER_CONTEXT, free_token_context},
	chain::{AliasState, CoinageChain, RingPosition},
	ledger::{self, BlockRef, CoinOnChain, EntryOnChain, Facts, Read, Subject, TokenId},
	namespace::{coin_key, entry_seed},
	plugins::{PersonhoodSource, Seed},
	tokens::period_of,
};
use crate::{chain::HashAndNumber, runtime::load_vrf, store::Db};

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
		Ok(found) => reads.extend(coins.iter().zip(found).map(|((asset, _), coin)| {
			let now = coin.filter(|coin| coin.instance == params.config().instance).and_then(|coin| {
				Some(CoinOnChain {
					exponent: Exponent::from_chain(coin.value, params)?,
					age: u32::from(coin.age),
					locked_until: coin.locked_until,
				})
			});
			(Subject::Asset(*asset), Read::Coin { now, emptied_by: None })
		})),
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
						}))
					},
				},
			};
			reads.push((Subject::Asset(*asset), read));
		}
	}

	if let Some((signer, ring, _)) = deps.personhood.person().await {
		let period = period_of(now, params);
		let tokens: Vec<(TokenId, [u8; 32])> = (0..params.config().unload_allowance)
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

	let number = u32::try_from(block.number).unwrap_or(u32::MAX);
	let facts = Facts { at: BlockRef { number, hash: block.hash.0 }, reads };
	let page_size = params.config().ring_page_size;
	deps.store.write(move |tx| ledger::observe(tx, &facts, now, page_size)).await?;
	Ok(())
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
