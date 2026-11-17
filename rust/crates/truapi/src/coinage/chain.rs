//! The coinage pallet as the layer uses it (design section 1, the chain): the
//! names the layer looks up in the runtime description, which A3's
//! `validate` checks, and [`CoinageChain`], which reads the pallet's storage
//! and the members pallet's recycler rings at a block.

use std::sync::Arc;

use parity_scale_codec::Encode;
use subxt::{
	dynamic::At,
	ext::{
		frame_decode::storage::{IntoDecodableValues, IntoEncodableValues},
		scale_decode::DecodeAsType,
		scale_value::{Value, ValueDef},
	},
	storage::dynamic,
	utils::{AccountId32, H256, MultiAddress},
};

use crate::{
	chain::{BlockBackend, ChainHeads, HashAndNumber, Heads, StorageEntryName, StorageWatchError, WatchedStorage},
	chain_runtime::RuntimeFailure,
};

/// The pallet's name in the runtime.
pub const PALLET: &str = "Coinage";

/// The transaction extension whose value is a coinage origin.
pub const EXTENSION: &str = "AsCoinage";

/// The extension that carries the sr25519 signature of a coin origin and of
/// an unpaid signed load.
pub const VERIFY_SIGNATURE: &str = "VerifyMultiSignature";

/// Splits a coin into coins of smaller exponents.
pub const SPLIT: &str = "split";
/// Moves a coin to a fresh account.
pub const TRANSFER: &str = "transfer";
/// Recycles a coin into a recycler entry.
pub const LOAD_WITH_COIN: &str = "load_recycler_with_coin";
/// Loads recycler entries from the external asset, paid by nobody.
pub const LOAD_UNPAID_BATCH: &str = "load_recycler_with_external_asset_unpaid_batch";
/// Unloads recycler entries into coins.
pub const UNLOAD_INTO_COINS: &str = "unload_recycler_into_coins";
/// Unloads recycler entries into the external asset.
pub const UNLOAD_INTO_EXTERNAL: &str = "unload_recycler_into_external_asset";
/// Unloads recycler entries into the external asset and reloads the surplus
/// into fresh entries in the same transaction (F3 invariant 3).
pub const UNLOAD_INTO_EXTERNAL_AND_LOADED: &str =
	"unload_recycler_into_external_asset_and_loaded_coins";

/// Every call the layer submits on the coinage pallet.
pub const CALLS: [&str; 7] = [
	SPLIT,
	TRANSFER,
	LOAD_WITH_COIN,
	LOAD_UNPAID_BATCH,
	UNLOAD_INTO_COINS,
	UNLOAD_INTO_EXTERNAL,
	UNLOAD_INTO_EXTERNAL_AND_LOADED,
];

/// The origin of a transaction signed by a coin's own key.
pub const AS_COIN: &str = "AsCoin";
/// The origin of an unload paid by a free token of a person.
pub const AS_TOKEN_PEOPLE: &str = "AsUnloadTokenPeople";
/// The origin of an unload paid by a free token of a lite person.
pub const AS_TOKEN_LITE_PEOPLE: &str = "AsUnloadTokenLitePeople";
/// The origin of a load from the external asset that nobody pays for.
pub const AS_UNPAID_SIGNED: &str = "InfallibleUnpaidSigned";

/// Every origin the layer uses, as variants of the extension's value.
pub const ORIGINS: [&str; 4] = [AS_COIN, AS_TOKEN_PEOPLE, AS_TOKEN_LITE_PEOPLE, AS_UNPAID_SIGNED];

/// The smallest legal exponent, an `i8`.
pub const MINIMUM_EXPONENT: &str = "MinimumExponent";
/// The largest legal exponent, an `i8`.
pub const MAXIMUM_EXPONENT: &str = "MaximumExponent";
/// The most coins one split, or one unload's reload, may create.
pub const MAX_SPLIT_OUTPUTS: &str = "MaxSplitOutputs";
/// The most entries one unload may present.
pub const MAX_CONSOLIDATION: &str = "MaxConsolidation";
/// The most entries one unpaid load may create.
pub const MAX_BATCH_UNPAID_LOAD: &str = "MaxBatchUnpaidLoad";
/// The size of a recycler ring, as a `RingExponent`.
pub const RECYCLER_RING_EXPONENT: &str = "RecyclerRingExponent";
/// The length of one free-token period, in seconds.
pub const TOKEN_PERIOD: &str = "UnloadTokenTimePeriodPeopleLitePeople";
/// The base of a coin's lock after a failed dispatch, in seconds.
pub const COIN_FAILURE_LOCK_PERIOD: &str = "CoinFailureLockPeriod";

/// The members pallet, which holds the recycler rings.
const MEMBERS: &str = "Members";

/// The pallet's storage of each alias's lock or unload.
const ALIAS_STATES: &str = "RecyclerAliasStates";

/// The identifier of the recycler ring collection for `exponent` of
/// `instance`: `coinage/recycler`, the instance little-endian, the exponent
/// as a byte, padded with zeros.
pub fn recycler_collection(instance: u32, exponent: i8) -> [u8; 32] {
	let mut identifier = [0u8; 32];
	identifier[..16].copy_from_slice(b"coinage/recycler");
	identifier[16..20].copy_from_slice(&instance.to_le_bytes());
	identifier[20] = exponent.to_le_bytes()[0];
	identifier
}

/// A coin as the pallet stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainCoin {
	/// Its instance.
	pub instance: u32,
	/// Its exponent, signed as the pallet stores it.
	pub value: i8,
	/// Its age.
	pub age: u16,
	/// When its lock after a failed dispatch ends, in Unix seconds.
	pub locked_until: Option<u64>,
}

/// An alias's state in a recycler ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AliasState {
	/// Locked by a failed dispatch until this time, in Unix seconds.
	Locked(u64),
	/// Consumed by a successful unload.
	Unloaded,
}

/// Where a member stands in its recycler ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingPlace {
	/// The ring's index.
	pub ring: u32,
	/// The page of the ring the member is on.
	pub page: u32,
	/// The member's position within the page.
	pub position: u32,
	/// How many members the ring has.
	pub members: u32,
	/// How many the ring's root includes.
	pub included: u32,
	/// When the ring stopped accepting members, in Unix seconds.
	pub immutable_since: Option<u64>,
	/// The ring root's revision.
	pub revision: u32,
}

/// Where a member key stands in its recycler collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingPosition {
	/// Queued to join a ring. It was never in one, so its entry was never
	/// unloaded.
	Onboarding,
	/// In a ring.
	Included(RingPlace),
	/// Taken out of its ring, or any other position the layer does not
	/// read: the pallet holds no ring index to read its alias under.
	Suspended,
}

/// The coinage pallet's storage, and the members pallet's recycler rings,
/// read at one block. Every read is batched over its keys, and a missing
/// value reads as none.
#[async_trait::async_trait]
pub trait CoinageChain: Send + Sync {
	/// The chain's finalized and best heads.
	async fn heads(&self) -> Result<Heads, StorageWatchError>;
	/// The coin at each account, with its lock.
	async fn coins_at(&self, at: H256, accounts: &[[u8; 32]]) -> Result<Vec<Option<ChainCoin>>, StorageWatchError>;
	/// The instance and exponent each member key was loaded under.
	async fn members_at(&self, at: H256, members: &[[u8; 32]]) -> Result<Vec<Option<(u32, i8)>>, StorageWatchError>;
	/// The position of each member key in the recycler collection of
	/// `exponent`; none for a key that is not a member.
	async fn places_at(
		&self,
		at: H256,
		exponent: i8,
		members: &[[u8; 32]],
	) -> Result<Vec<Option<RingPosition>>, StorageWatchError>;
	/// The state of each alias in ring `ring` of `exponent`, none for an
	/// alias the pallet holds nothing for. A held value that is not an
	/// `AliasState` fails the read, so it is never taken for a free alias.
	async fn aliases_at(
		&self,
		at: H256,
		exponent: i8,
		ring: u32,
		aliases: &[[u8; 32]],
	) -> Result<Vec<Option<AliasState>>, StorageWatchError>;
	/// The members of ring `ring` of `exponent`, in ring order, up to its
	/// included count.
	async fn ring_members_at(&self, at: H256, exponent: i8, ring: u32) -> Result<Vec<[u8; 32]>, StorageWatchError>;
	/// Whether each free token identity, a period and the person's alias in
	/// it, is recorded as consumed.
	async fn tokens_consumed_at(&self, at: H256, tokens: &[(u32, [u8; 32])]) -> Result<Vec<bool>, StorageWatchError>;
	/// An account's next nonce.
	async fn nonce_at(&self, at: H256, account: [u8; 32]) -> Result<u32, StorageWatchError>;
	/// What an account holds of the instance's external asset, in the
	/// asset's units; zero for an account that holds none.
	async fn funds_at(&self, at: H256, account: [u8; 32]) -> Result<u128, StorageWatchError>;
	/// The hash of the transaction that emptied a coin's account, and the
	/// number of the block that included it, given the account held the
	/// coin at block `since` and is empty at `at`; none when no transaction
	/// that coin signed is found there.
	async fn emptied_by(
		&self,
		account: [u8; 32],
		since: u64,
		at: HashAndNumber,
	) -> Result<Option<Emptying>, StorageWatchError>;
}

/// The transaction that emptied a coin's account.
#[derive(Clone, Copy)]
pub struct Emptying {
	/// Its hash.
	pub transaction: [u8; 32],
	/// The number of the block that included it.
	pub block: u64,
}

/// [`CoinageChain`] over the storage, heads and blocks of the People chain.
pub struct CoinageStorage {
	storage: Arc<dyn WatchedStorage>,
	heads: Arc<dyn ChainHeads>,
	blocks: Arc<dyn BlockBackend>,
	genesis: H256,
	instance: u32,
}

impl CoinageStorage {
	/// Reads of the pallet's `instance` on the chain with `genesis`.
	pub fn new(
		storage: Arc<dyn WatchedStorage>,
		heads: Arc<dyn ChainHeads>,
		blocks: Arc<dyn BlockBackend>,
		genesis: H256,
		instance: u32,
	) -> Self {
		Self { storage, heads, blocks, genesis, instance }
	}

	/// The block at `number`, canonical up to the finalized height.
	async fn block_at(&self, number: u64) -> Result<Option<HashAndNumber>, StorageWatchError> {
		Ok(self.blocks.block_hash(self.genesis, number).await?.map(|hash| HashAndNumber { hash, number }))
	}

	/// The values of `pallet::entry` at `at` for each of `keys`, decoded as
	/// dynamic values.
	async fn values<K>(
		&self,
		at: H256,
		pallet: &'static str,
		entry: &'static str,
		keys: &[K],
	) -> Result<Vec<Option<Value>>, StorageWatchError>
	where
		K: IntoEncodableValues + IntoDecodableValues + Clone,
	{
		self.storage.read_entries_at(self.genesis, at, &dynamic::<K, Value>(pallet, entry), keys).await
	}

	/// The status and the root revision of ring `ring` of `collection`.
	async fn ring_status(&self, at: H256, collection: [u8; 32], ring: u32) -> Result<(Option<Value>, u32), StorageWatchError> {
		let status = self.values(at, MEMBERS, "RingKeysStatus", &[(collection, ring)]).await?.into_iter().flatten().next();
		let root = self.values(at, MEMBERS, "Root", &[(collection, ring)]).await?.into_iter().flatten().next();
		let revision = root.as_ref().and_then(|root| number(root, "revision")).unwrap_or(0) as u32;
		Ok((status, revision))
	}
}

#[derive(DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
struct StoredCoin {
	instance_id: u32,
	value: i8,
	age: u16,
}

#[async_trait::async_trait]
impl CoinageChain for CoinageStorage {
	async fn heads(&self) -> Result<Heads, StorageWatchError> {
		Ok(self.heads.heads(self.genesis).await?)
	}

	async fn coins_at(&self, at: H256, accounts: &[[u8; 32]]) -> Result<Vec<Option<ChainCoin>>, StorageWatchError> {
		let owners: Vec<(AccountId32,)> = accounts.iter().map(|account| (AccountId32(*account),)).collect();
		let coins = self
			.storage
			.read_entries_at(self.genesis, at, &dynamic::<(AccountId32,), StoredCoin>(PALLET, "CoinsByOwner"), &owners)
			.await?;
		let locks = self.values(at, PALLET, "LockedCoins", &owners).await?;
		Ok(coins
			.into_iter()
			.zip(locks)
			.map(|(coin, lock)| {
				coin.map(|coin| ChainCoin {
					instance: coin.instance_id,
					value: coin.value,
					age: coin.age,
					locked_until: lock.as_ref().and_then(|lock| number(lock, "until")).map(|until| until as u64),
				})
			})
			.collect())
	}

	async fn members_at(&self, at: H256, members: &[[u8; 32]]) -> Result<Vec<Option<(u32, i8)>>, StorageWatchError> {
		let keys: Vec<([u8; 32],)> = members.iter().map(|member| (*member,)).collect();
		self.storage
			.read_entries_at(self.genesis, at, &dynamic::<([u8; 32],), (u32, i8)>(PALLET, "RecyclersCoinToRecycler"), &keys)
			.await
	}

	async fn places_at(
		&self,
		at: H256,
		exponent: i8,
		members: &[[u8; 32]],
	) -> Result<Vec<Option<RingPosition>>, StorageWatchError> {
		let collection = recycler_collection(self.instance, exponent);
		let keys: Vec<([u8; 32], [u8; 32])> = members.iter().map(|member| (collection, *member)).collect();
		let positions = self.values(at, MEMBERS, "Members", &keys).await?;
		let mut places = Vec::with_capacity(positions.len());
		for position in positions {
			let Some(position) = position else {
				places.push(None);
				continue;
			};
			if variant_name(&position) == Some("Onboarding") {
				places.push(Some(RingPosition::Onboarding));
				continue;
			}
			let Some((ring, page, slot)) = included_position(&position) else {
				places.push(Some(RingPosition::Suspended));
				continue;
			};
			let (status, revision) = self.ring_status(at, collection, ring).await?;
			let field = |name: &str| status.as_ref().and_then(|status| number(status, name));
			let immutable_since = status.as_ref().and_then(|status| status.at("immutable_since")).and_then(|since| since.at(0));
			places.push(Some(RingPosition::Included(RingPlace {
				ring,
				page,
				position: slot,
				members: field("total").unwrap_or(0) as u32,
				included: field("included").unwrap_or(0) as u32,
				immutable_since: immutable_since.and_then(Value::as_u128).map(|since| since as u64),
				revision,
			})));
		}
		Ok(places)
	}

	async fn aliases_at(
		&self,
		at: H256,
		exponent: i8,
		ring: u32,
		aliases: &[[u8; 32]],
	) -> Result<Vec<Option<AliasState>>, StorageWatchError> {
		let keys: Vec<(u32, i8, u32, [u8; 32])> =
			aliases.iter().map(|alias| (self.instance, exponent, ring, *alias)).collect();
		let states = self.values(at, PALLET, ALIAS_STATES, &keys).await?;
		states
			.iter()
			.map(|state| match state {
				None => Ok(None),
				Some(value) => alias_state(value).map(Some).ok_or_else(|| StorageWatchError::Incompatible {
					entry: StorageEntryName { pallet: PALLET.to_owned(), entry: ALIAS_STATES.to_owned() },
					reason: "a stored value is not an AliasState".to_owned(),
				}),
			})
			.collect()
	}

	async fn ring_members_at(&self, at: H256, exponent: i8, ring: u32) -> Result<Vec<[u8; 32]>, StorageWatchError> {
		let collection = recycler_collection(self.instance, exponent);
		let (status, _) = self.ring_status(at, collection, ring).await?;
		let included = status.as_ref().and_then(|status| number(status, "included")).unwrap_or(0) as usize;
		let mut members = Vec::new();
		let mut page = 0u32;
		while members.len() < included {
			let address = dynamic::<([u8; 32], u32, u32), Vec<[u8; 32]>>(MEMBERS, "RingKeys");
			let keys = self.storage.read_entries_at(self.genesis, at, &address, &[(collection, ring, page)]).await?;
			let Some(keys) = keys.into_iter().flatten().next().filter(|keys| !keys.is_empty()) else { break };
			members.extend(keys);
			page += 1;
		}
		members.truncate(included);
		Ok(members)
	}

	async fn tokens_consumed_at(&self, at: H256, tokens: &[(u32, [u8; 32])]) -> Result<Vec<bool>, StorageWatchError> {
		let consumed = self.values(at, PALLET, "ConsumedFreeUnloadTokens", tokens).await?;
		Ok(consumed.into_iter().map(|entry| entry.is_some()).collect())
	}

	async fn nonce_at(&self, at: H256, account: [u8; 32]) -> Result<u32, StorageWatchError> {
		let info = self.values(at, "System", "Account", &[(AccountId32(account),)]).await?;
		Ok(info.into_iter().flatten().next().as_ref().and_then(|info| number(info, "nonce")).unwrap_or(0) as u32)
	}

	async fn funds_at(&self, at: H256, account: [u8; 32]) -> Result<u128, StorageWatchError> {
		let instance = self.values(at, PALLET, "Instances", &[(self.instance,)]).await?.into_iter().flatten().next();
		let Some(asset) = instance.as_ref().and_then(|record| record.at("asset_id")).cloned() else { return Ok(0) };
		let held = self.values(at, "Assets", "Account", &[(asset, AccountId32(account))]).await?;
		Ok(held.into_iter().flatten().next().as_ref().and_then(|held| number(held, "balance")).unwrap_or(0))
	}

	async fn emptied_by(
		&self,
		account: [u8; 32],
		since: u64,
		at: HashAndNumber,
	) -> Result<Option<Emptying>, StorageWatchError> {
		let (mut held, mut emptied) = (since, at);
		while emptied.number > held + 1 {
			let middle = held + (emptied.number - held) / 2;
			let Some(block) = self.block_at(middle).await? else { return Ok(None) };
			match self.coins_at(block.hash, &[account]).await?.first() {
				Some(Some(_)) => held = middle,
				_ => emptied = block,
			}
		}
		let failure = |error: subxt::Error| RuntimeFailure::host_failure("coinage_emptied_by", error.to_string());
		let client = self.storage.client_at(self.genesis, emptied).await?;
		let extrinsics = client.extrinsics().fetch().await.map_err(|error| failure(error.into()))?;
		let signer = MultiAddress::<AccountId32, ()>::Id(AccountId32(account)).encode();
		for extrinsic in extrinsics.iter() {
			let extrinsic = extrinsic.map_err(|error| failure(error.into()))?;
			if extrinsic.address_bytes() == Some(signer.as_slice()) {
				return Ok(Some(Emptying { transaction: extrinsic.hash().0, block: emptied.number }));
			}
		}
		Ok(None)
	}
}

/// The unsigned number in the named field of a composite value.
fn number(value: &Value, field: &str) -> Option<u128> {
	value.at(field).and_then(Value::as_u128)
}

/// An alias's state, from the pallet's `AliasState`; none when the value is
/// not one, such as a lock without its end.
fn alias_state(state: &Value) -> Option<AliasState> {
	let variant = variant_name(state)?;
	match variant {
		"Unloaded" => Some(AliasState::Unloaded),
		"Locked" => {
			let until = state.at(0).and_then(|lock| number(lock, "until"))?;
			Some(AliasState::Locked(u64::try_from(until).ok()?))
		},
		_ => None,
	}
}

/// The ring, page and position of an `Included` ring position.
fn included_position(position: &Value) -> Option<(u32, u32, u32)> {
	if variant_name(position)? != "Included" {
		return None;
	}
	let field = |name: &str| number(position, name).map(|value| value as u32);
	Some((field("ring_index")?, field("ring_page")?, field("ring_position")?))
}

/// The variant name of an enum value.
fn variant_name(value: &Value) -> Option<&str> {
	match &value.value {
		ValueDef::Variant(variant) => Some(variant.name.as_str()),
		_ => None,
	}
}
