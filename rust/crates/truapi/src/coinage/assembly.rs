//! E2 · Transaction assembly: one planned transaction as signed bytes, with
//! its call, its origin, its proofs, its validity window and its signature.
//!
//! Each origin has exactly one construction path here (invariant 1): a coin
//! signs as itself, an unload presents entry proofs and a person's free
//! token, and a load is signed by its funding account. Assembly reads nothing:
//! the ring, the person's ring and the nonce arrive in a [`Context`] read
//! beforehand, so equal arguments give equal bytes, up to the randomness of
//! the signatures.

use core::sync::atomic::{AtomicBool, Ordering};

use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use subxt::{
	config::substrate::SubstrateConfig,
	ext::scale_encode::EncodeAsType,
	tx::Signer,
	utils::{AccountId32, H256, MultiSignature},
};

use super::{
	Asset, ChainParams, CoinSecret, Exponent,
	ledger::TokenId,
	namespace::{coin_key, entry_seed},
	planning::Call,
};
use crate::{
	chain::{
		ExtensionError, ExtrinsicBuildError, ExtrinsicService, ExtrinsicVersion, HashAndNumber,
		InheritedImplication, MortalExtrinsic, TransactionExtension,
	},
	host_internal::extrinsic::Sr25519Signer,
	runtime::{
		Vrf,
		statement_allowance::{PersonhoodSigner, collection::PersonhoodCollection, ring::RingParams},
	},
};

/// The context an unload's entry proofs are built in.
pub const UNLOADING_RECYCLER_CONTEXT: [u8; 32] = *b"pop:polkadot.network/coinrecyclr";

const FREE_TOKEN_CONTEXT_PREFIX: &[u8; 24] = b"pop:polkadot.net/coinftk";

/// One transaction to build: the call, and its inputs and outputs as C1's
/// rows name them, with their denominations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
	/// The call.
	pub call: Call,
	/// What it consumes.
	pub inputs: Vec<(Asset, Exponent)>,
	/// What it creates.
	pub outputs: Vec<(Asset, Exponent)>,
	/// The unload token it presents.
	pub token: Option<TokenId>,
}

/// A recycler ring at one revision, with its members in ring order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ring {
	/// The ring's index.
	pub index: u32,
	/// The revision the proofs are built against.
	pub revision: u32,
	/// Its members, in ring order, up to the revision's included count.
	pub members: Vec<[u8; 32]>,
}

/// What assembly needs and cannot compute, read once per transaction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
	/// For an unload, the ring holding its entries.
	pub ring: Option<Ring>,
	/// For a load, the funding account's next nonce.
	pub nonce: Option<u32>,
}

/// A person's ring, for an unload's free token.
pub struct Personhood<'a> {
	/// Holds the person's keys.
	pub signer: &'a dyn PersonhoodSigner,
	/// The person's ring.
	pub ring: &'a RingParams,
	/// The ring root revision the proof is built against.
	pub revision: u32,
}

/// Signs a top-up for its funding account. The account's secrets never
/// enter the layer (G1 invariant 6).
pub trait Funding: Send + Sync {
	/// The account that pays.
	fn account(&self) -> [u8; 32];
	/// An sr25519 signature over `payload`, or none when the signer declines.
	fn sign(&self, payload: &[u8]) -> Option<[u8; 64]>;
}

/// A funding account whose keypair is held in memory: a product account of
/// this wallet, or a one-time account the product handed over.
pub struct KeypairFunding(pub schnorrkel::Keypair);

impl Funding for KeypairFunding {
	fn account(&self) -> [u8; 32] {
		self.0.public.to_bytes()
	}

	fn sign(&self, payload: &[u8]) -> Option<[u8; 64]> {
		Some(self.0.sign_simple(b"substrate", payload).to_bytes())
	}
}

/// Where the keys of a transaction come from.
pub struct Keys<'a> {
	/// The recovery phrase's entropy, for the layer's own coins and entries.
	pub entropy: &'a [u8],
	/// Ring-VRF operations.
	pub vrf: &'a Vrf,
	/// The secret of a peer's coin being imported.
	pub imported: Option<&'a CoinSecret>,
	/// The funding account of a top-up.
	pub funding: Option<std::sync::Arc<dyn Funding>>,
	/// The person whose free token an unload presents.
	pub person: Option<Personhood<'a>>,
}

/// Why a transaction could not be built.
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum AssemblyError {
	/// A key could not be derived or used.
	#[display("a key is unusable: {_0}")]
	Key(#[error(not(source))] String),
	/// The context or the keys lack what this transaction needs.
	#[display("missing {_0}")]
	Missing(#[error(not(source))] &'static str),
	/// The extrinsic builder failed.
	Build(ExtrinsicBuildError),
}

/// Where and with which validity window a transaction is built.
pub struct Build<'a> {
	/// Builds the extrinsic.
	pub service: &'a ExtrinsicService,
	/// The chain it is built for.
	pub genesis: H256,
	/// Its birth block.
	pub birth: HashAndNumber,
	/// How many blocks it stays valid for.
	pub period: u32,
}

/// The signed bytes of `tx`, or none when a funding signer declined.
pub async fn assemble(
	build: &Build<'_>,
	tx: &Transaction,
	context: &Context,
	keys: &Keys<'_>,
	params: &ChainParams,
) -> Result<Option<MortalExtrinsic>, AssemblyError> {
	let Build { service, genesis, birth, period } = *build;
	let service = service.cache_runtime_at(genesis, birth).await?;
	let version = ExtrinsicVersion::V5 { extension_version: params.config().extension_version };
	let builder = service
		.build_mortal_extrinsic(genesis)
		.with_mortality_period(u64::from(period))
		.with_extrinsic_version(version);
	let instance = params.config().instance;
	match tx.call {
		Call::Split | Call::Transfer | Call::Recycle => {
			let (input, _) = tx.inputs.first().ok_or(AssemblyError::Missing("an input coin"))?;
			let signer = coin_signer(input, keys)?;
			let coin_account = signer.account_id().0;
			let builder = match tx.call {
				Call::Split => builder.with_call_args("Coinage", "split", SplitArgs { split_into: split_into(tx, keys)? }),
				Call::Transfer => {
					let (output, _) = tx.outputs.first().ok_or(AssemblyError::Missing("an output coin"))?;
					builder.with_call_args("Coinage", "transfer", TransferArgs { to: coin_account_of(output, keys)? })
				},
				_ => {
					let (output, _) = tx.outputs.first().ok_or(AssemblyError::Missing("an output entry"))?;
					let seed = entry_seed_of(output, keys)?;
					let args = LoadWithCoinArgs {
						member_key: member_of(&seed, keys)?,
						proof_of_ownership: signature(keys.vrf.sign(&seed, &coin_account.encode()).map_err(key_error)?)?,
					};
					builder.with_call_args("Coinage", "load_recycler_with_coin", args)
				},
			};
			let built = builder.with_extension(AsCoinageExtension::Coin).with_signer(&signer).with_nonce(0).build().await?;
			Ok(Some(built))
		},
		Call::Load(from) => {
			let funding = keys.funding.clone().ok_or(AssemblyError::Missing("a funding signer"))?;
			let nonce = context.nonce.ok_or(AssemblyError::Missing("the funding account's nonce"))?;
			let items = tx
				.outputs
				.iter()
				.map(|(output, exponent)| {
					let seed = entry_seed_of(output, keys)?;
					Ok(UnpaidLoadInput {
						preservation: CodecPreservation::Preserve,
						value: chain_exponent(*exponent),
						member_key: member_of(&seed, keys)?,
						proof_of_ownership: signature(keys.vrf.sign(&seed, &from.encode()).map_err(key_error)?)?,
					})
				})
				.collect::<Result<Vec<_>, AssemblyError>>()?;
			let signer = FundingSigner { funding, declined: AtomicBool::new(false) };
			let built = builder
				.with_call_args("Coinage", "load_recycler_with_external_asset_unpaid_batch", UnpaidBatchArgs {
					instance_id: instance,
					items,
				})
				.with_extension(AsCoinageExtension::Unpaid { nonce })
				.with_signer(&signer)
				.with_nonce(u64::from(nonce))
				.build()
				.await?;
			Ok((!signer.declined.load(Ordering::SeqCst)).then_some(built))
		},
		Call::UnloadToCoins | Call::UnloadToExternal(_) => {
			let ring = context.ring.as_ref().ok_or(AssemblyError::Missing("the entries' ring"))?;
			let person = keys.person.as_ref().ok_or(AssemblyError::Missing("the person's ring"))?;
			let token = tx.token.ok_or(AssemblyError::Missing("an unload token"))?;
			let (_, value) = tx.inputs.first().ok_or(AssemblyError::Missing("an input entry"))?;
			let seeds = tx.inputs.iter().map(|(entry, _)| entry_seed_of(entry, keys)).collect::<Result<Vec<_>, _>>()?;
			let aliases = seeds
				.iter()
				.map(|seed| keys.vrf.alias(seed, &UNLOADING_RECYCLER_CONTEXT).map_err(key_error))
				.collect::<Result<Vec<_>, _>>()?;
			let unload = UnloadProofs {
				vrf: keys.vrf,
				ring,
				domain: crate::runtime::statement_allowance::proof::domain_for_ring_exponent(params.ring_exponent())
					.map_err(|error| AssemblyError::Key(error.to_string()))?,
				seeds,
				person,
				token,
			};
			let common = (instance, aliases, chain_exponent(*value), ring.index, ring.revision);
			let builder = match tx.call {
				Call::UnloadToExternal(to) => {
					let change: Vec<(Asset, Exponent)> = tx.outputs.clone();
					let input_value: u64 = tx.inputs.iter().map(|(_, exponent)| exponent.value()).sum();
					let change_value: u64 = change.iter().map(|(_, exponent)| exponent.value()).sum();
					if change.is_empty() {
						builder.with_call_args("Coinage", "unload_recycler_into_external_asset", UnloadToExternalArgs {
							instance_id: common.0,
							aliases: common.1,
							value: common.2,
							index: common.3,
							revision: common.4,
							to,
							max_fee: 0,
						})
					} else {
						let loaded_coins = change
							.iter()
							.map(|(entry, exponent)| Ok((chain_exponent(*exponent), member_of(&entry_seed_of(entry, keys)?, keys)?)))
							.collect::<Result<Vec<_>, AssemblyError>>()?;
						let external = u128::from(input_value - change_value) * params.instance().asset_unit;
						builder.with_call_args(
							"Coinage",
							"unload_recycler_into_external_asset_and_loaded_coins",
							UnloadToExternalAndLoadedArgs {
								instance_id: common.0,
								aliases: common.1,
								value: common.2,
								index: common.3,
								revision: common.4,
								to,
								external_asset_amount: external,
								loaded_coins,
								max_fee: 0,
							},
						)
					}
				},
				_ => builder.with_call_args("Coinage", "unload_recycler_into_coins", UnloadToCoinsArgs {
					instance_id: common.0,
					aliases: common.1,
					value: common.2,
					index: common.3,
					revision: common.4,
					split_into: split_into(tx, keys)?,
					max_fee: 0,
				}),
			};
			let built = builder.with_extension(AsCoinageExtension::Token(unload)).build().await?;
			Ok(Some(built))
		},
		Call::AppendRegistryRecord(_) => Err(AssemblyError::Missing("a registry builder for this call")),
	}
}

/// `split_into`: the output coins grouped by exponent, ascending, each group
/// with its accounts.
fn split_into(tx: &Transaction, keys: &Keys<'_>) -> Result<Vec<SplitGroup>, AssemblyError> {
	let mut groups: std::collections::BTreeMap<Exponent, Vec<[u8; 32]>> = Default::default();
	for (output, exponent) in &tx.outputs {
		groups.entry(*exponent).or_default().push(coin_account_of(output, keys)?);
	}
	Ok(groups.into_iter().map(|(exponent, accounts)| SplitGroup(chain_exponent(exponent), accounts)).collect())
}

fn chain_exponent(exponent: Exponent) -> i8 {
	i8::try_from(exponent.get()).unwrap_or(i8::MAX)
}

fn coin_signer(input: &Asset, keys: &Keys<'_>) -> Result<Sr25519Signer, AssemblyError> {
	let secret = match input {
		Asset::Coin { purse, installation, index } => {
			coin_key(keys.entropy, *purse, *installation, *index).map_err(key_error)?.0
		},
		Asset::PeerCoin(_) => keys.imported.cloned().ok_or(AssemblyError::Missing("the peer coin's secret"))?,
		Asset::Entry { .. } => return Err(AssemblyError::Missing("a coin input")),
	};
	Sr25519Signer::from_secret_bytes(&secret.0).map_err(AssemblyError::Key)
}

fn coin_account_of(asset: &Asset, keys: &Keys<'_>) -> Result<[u8; 32], AssemblyError> {
	match asset {
		Asset::Coin { purse, installation, index } => Ok(coin_key(keys.entropy, *purse, *installation, *index).map_err(key_error)?.1),
		_ => Err(AssemblyError::Missing("a coin output")),
	}
}

fn entry_seed_of(asset: &Asset, keys: &Keys<'_>) -> Result<[u8; 32], AssemblyError> {
	match asset {
		Asset::Entry { purse, installation, index } => Ok(entry_seed(keys.entropy, *purse, *installation, *index).0),
		_ => Err(AssemblyError::Missing("an entry")),
	}
}

fn member_of(seed: &[u8; 32], keys: &Keys<'_>) -> Result<[u8; 32], AssemblyError> {
	keys.vrf.member(seed).map_err(key_error)
}

fn signature(bytes: Vec<u8>) -> Result<[u8; 64], AssemblyError> {
	<[u8; 64]>::try_from(bytes.as_slice()).map_err(|_| AssemblyError::Key("a Bandersnatch signature is 64 bytes".into()))
}

fn key_error(error: impl core::fmt::Debug) -> AssemblyError {
	AssemblyError::Key(format!("{error:?}"))
}

/// The context of a person's free token in `period` at `counter`.
pub fn free_token_context(token: TokenId) -> [u8; 32] {
	let mut context = [0u8; 32];
	context[..24].copy_from_slice(FREE_TOKEN_CONTEXT_PREFIX);
	context[24..28].copy_from_slice(&token.period.to_le_bytes());
	context[28..].copy_from_slice(&token.slot.to_le_bytes());
	context
}

/// What an unload's origin proves: each entry's alias over the
/// implication's hash, and the person's free token over the entry proofs and
/// the implication.
struct UnloadProofs<'a> {
	vrf: &'a Vrf,
	ring: &'a Ring,
	domain: u32,
	seeds: Vec<[u8; 32]>,
	person: &'a Personhood<'a>,
	token: TokenId,
}

/// The pallet's `AsCoinage` extension, one construction path per origin.
enum AsCoinageExtension<'a> {
	Coin,
	Unpaid { nonce: u32 },
	Token(UnloadProofs<'a>),
}

/// The pallet's `MembershipProof`.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct MembershipProof {
	proof: Vec<u8>,
	ring: u32,
	revision: u32,
}

/// The pallet's `AsCoinageInfo`, by variant name.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
enum AsCoinageInfo {
	AsCoin,
	AsUnloadTokenPeople { proof: MembershipProof, period: u32, counter: u32, alias_proofs: Vec<Vec<u8>> },
	AsUnloadTokenLitePeople { proof: MembershipProof, period: u32, counter: u32, alias_proofs: Vec<Vec<u8>> },
	InfallibleUnpaidSigned { nonce: u32 },
}

impl TransactionExtension for AsCoinageExtension<'_> {
	type Implicit = ();
	type Explicit = Option<AsCoinageInfo>;

	fn name(&self) -> &str {
		"AsCoinage"
	}

	fn implicit(&self) {}

	fn explicit(&self, implication: &InheritedImplication) -> Result<Option<AsCoinageInfo>, ExtensionError> {
		Ok(Some(match self {
			Self::Coin => AsCoinageInfo::AsCoin,
			Self::Unpaid { nonce } => AsCoinageInfo::InfallibleUnpaidSigned { nonce: *nonce },
			Self::Token(unload) => unload.origin(implication)?,
		}))
	}
}

impl UnloadProofs<'_> {
	fn origin(&self, implication: &InheritedImplication) -> Result<AsCoinageInfo, ExtensionError> {
		let proven = implication.hash();
		let alias_proofs = self
			.seeds
			.iter()
			.map(|seed| {
				let member = self.vrf.member(seed).map_err(|error| ExtensionError::new(key_error(error)))?;
				let (proof, _) = self
					.vrf
					.prove(seed, self.domain, &member, &self.ring.members, &UNLOADING_RECYCLER_CONTEXT, &proven)
					.map_err(|error| ExtensionError::new(key_error(error)))?;
				Ok(proof)
			})
			.collect::<Result<Vec<_>, ExtensionError>>()?;
		let mut message = alias_proofs.encode();
		message.extend_from_slice(implication.encoded());
		let context = free_token_context(self.token);
		let token_proof =
			self.person.signer.prove(self.person.ring, &context, &blake2_256(&message)).map_err(|error| ExtensionError::new(key_error(error)))?;
		let proof = MembershipProof { proof: token_proof, ring: self.person.ring.ring_index, revision: self.person.revision };
		let (period, counter) = (self.token.period, self.token.slot);
		Ok(match self.person.ring.collection {
			PersonhoodCollection::LitePeople => AsCoinageInfo::AsUnloadTokenLitePeople { proof, period, counter, alias_proofs },
			_ => AsCoinageInfo::AsUnloadTokenPeople { proof, period, counter, alias_proofs },
		})
	}
}

/// A funding capability as a subxt signer. A declined signature is
/// recorded, and the bytes built with it are discarded.
struct FundingSigner {
	funding: std::sync::Arc<dyn Funding>,
	declined: AtomicBool,
}

impl Signer<SubstrateConfig> for FundingSigner {
	fn account_id(&self) -> AccountId32 {
		AccountId32(self.funding.account())
	}

	fn sign(&self, payload: &[u8]) -> MultiSignature {
		match self.funding.sign(payload) {
			Some(signature) => MultiSignature::Sr25519(signature),
			None => {
				self.declined.store(true, Ordering::SeqCst);
				MultiSignature::Sr25519([0; 64])
			},
		}
	}
}

#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct SplitArgs {
	split_into: Vec<SplitGroup>,
}

#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct TransferArgs {
	to: [u8; 32],
}

#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct LoadWithCoinArgs {
	member_key: [u8; 32],
	proof_of_ownership: [u8; 64],
}

#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
enum CodecPreservation {
	Preserve,
}

#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct UnpaidLoadInput {
	preservation: CodecPreservation,
	value: i8,
	member_key: [u8; 32],
	proof_of_ownership: [u8; 64],
}

#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct UnpaidBatchArgs {
	instance_id: u32,
	items: Vec<UnpaidLoadInput>,
}

#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct UnloadToCoinsArgs {
	instance_id: u32,
	aliases: Vec<[u8; 32]>,
	value: i8,
	index: u32,
	revision: u32,
	split_into: Vec<SplitGroup>,
	max_fee: u128,
}

#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct UnloadToExternalArgs {
	instance_id: u32,
	aliases: Vec<[u8; 32]>,
	value: i8,
	index: u32,
	revision: u32,
	to: [u8; 32],
	max_fee: u128,
}

#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct UnloadToExternalAndLoadedArgs {
	instance_id: u32,
	aliases: Vec<[u8; 32]>,
	value: i8,
	index: u32,
	revision: u32,
	to: [u8; 32],
	external_asset_amount: u128,
	loaded_coins: Vec<(i8, [u8; 32])>,
	max_fee: u128,
}

#[cfg(test)]
mod tests;

/// One group of a split's outputs: the exponent and the accounts that
/// receive a coin of it.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct SplitGroup(i8, Vec<[u8; 32]>);
