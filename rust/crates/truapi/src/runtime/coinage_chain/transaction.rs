// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer
// core/crates/brevity-ffi/src/{coinage_sender,coinage_transfer}.rs. Copyright the Brevity
// contributors. See truapi-coinage/NOTICE and LICENSE.

use crate::{
	chain::{
		EncodedExtrinsic, ExtensionError, ExtrinsicBuilder, ExtrinsicService, ExtrinsicVersion,
		HashAndNumber, InheritedImplication, RuntimeAtBirth, TransactionExtension,
	},
	host_internal::extrinsic::Sr25519Signer,
	runtime::statement_allowance::extension::Metadata,
};
use parity_scale_codec::{Decode, Encode};
use subxt::ext::{
	frame_decode::extrinsics::{ExtrinsicCallInfo, ExtrinsicTypeInfo},
	scale_encode::{EncodeAsFields, EncodeAsType, Error as EncodeError, FieldIter, TypeResolver},
};
use truapi_coinage::DenominationBreakdownContext;

pub(super) const AS_COINAGE: &str = "AsCoinage";

#[derive(Clone)]
pub(super) struct Snapshot {
	pub storage: std::sync::Arc<StorageLayouts>,
	pub metadata: std::sync::Arc<Metadata>,
	pub subxt: subxt::metadata::ArcMetadata,
	pub genesis_hash: [u8; 32],
	pub spec_version: u32,
	pub transaction_version: u32,
	pub at: [u8; 32],
	pub number: u64,
	pub period: u64,
	asset_unit: Option<u128>,
}

impl Snapshot {
	pub fn new(
		raw: Vec<u8>,
		at: [u8; 32],
		number: u64,
		genesis_hash: [u8; 32],
		spec_version: u32,
		transaction_version: u32,
		configured_instance: Option<u32>,
	) -> Result<Self, String> {
		let metadata = Metadata::decode(&raw).map_err(|_| "invalid Coinage runtime metadata")?;
		let subxt = subxt::Metadata::decode_from(&raw[..])
			.map_err(|_| "invalid transaction metadata")?
			.arc();
		let hashes: u32 = constant(&metadata, "System", "BlockHashCount")?;
		let limit = u64::from(hashes).min(truapi_coinage::WAL_MORTALITY_BLOCKS).min(256);
		if limit < 4 {
			return Err("insufficient block retention for mortal Coinage transactions".into());
		}
		let period = 1u64 << (63 - limit.leading_zeros());
		let storage = StorageLayouts::new(subxt.clone(), configured_instance)?;
		let asset_unit = if storage.instance_id.is_none() {
			Some(constant(&metadata, "Coinage", "UnderlyingAssetUnit")?)
		} else {
			None
		};
		let snapshot = Self {
			storage: std::sync::Arc::new(storage),
			metadata: std::sync::Arc::new(metadata),
			subxt,
			genesis_hash,
			spec_version,
			transaction_version,
			at,
			number,
			period,
			asset_unit,
		};
		if snapshot.asset_unit.is_some() {
			snapshot.denomination_context()?;
		}
		Ok(snapshot)
	}

	pub fn instance_asset_key(&self) -> Result<Option<Vec<u8>>, String> {
		self.storage
			.instance_id
			.map(|instance| {
				self.storage.typed_key("Coinage", "Instances", &[&instance.to_le_bytes()])
			})
			.transpose()
	}

	/// Instance configuration is mutable storage, not runtime metadata. Re-read
	/// it at every queried head, even when the runtime version is unchanged.
	pub fn bind_instance_asset(&mut self, bytes: &[u8]) -> Result<(), String> {
		#[derive(scale_decode::DecodeAsType)]
		struct InstanceRecord {
			asset_unit: u128,
		}
		let record: InstanceRecord = self.storage.decode_value("Coinage", "Instances", bytes)?;
		self.asset_unit = Some(record.asset_unit);
		self.denomination_context()?;
		Ok(())
	}

	pub fn valid_until(&self) -> u64 {
		self.number.saturating_add(self.period)
	}

	pub fn denomination_context(&self) -> Result<DenominationBreakdownContext, String> {
		let context = DenominationBreakdownContext {
			asset_unit: self
				.asset_unit
				.ok_or("Coinage asset configuration has not been read at this block")?,
			max_exponent: i16::from(constant::<i8>(&self.metadata, "Coinage", "MaximumExponent")?),
			min_exponent: i16::from(constant::<i8>(&self.metadata, "Coinage", "MinimumExponent")?),
			precision: truapi_coinage::CASH_ASSET_PRECISION,
		};
		if context.asset_unit == 0 || context.max_exponent < context.min_exponent {
			return Err("invalid Coinage denomination constants".into());
		}
		for exponent in context.min_exponent..=context.max_exponent {
			denomination_value(&context, exponent)?;
		}
		Ok(context)
	}

	pub fn as_coin(&self) -> Result<Vec<u8>, String> {
		let variant = self.authorization_variant("AsCoin")?;
		if !variant.fields.is_empty() {
			return Err("invalid AsCoinage.AsCoin fields".into());
		}
		Ok(vec![1, variant.index])
	}

	/// Resolve the authorization from the selected pipeline's explicit type,
	/// not from a separate metadata facade. AsCoinage is Option<Info>, possibly
	/// inside a single-field struct; neither wrapper may carry extra fields.
	fn authorization_variant(
		&self,
		name: &str,
	) -> Result<&scale_info::Variant<scale_info::form::PortableForm>, String> {
		use scale_info::TypeDef;

		let pipeline = self
			.subxt
			.extrinsic_extension_info(Some(self.metadata.extension_version()))
			.map_err(|_| "invalid extension pipeline")?;
		let extension = pipeline
			.extension_ids
			.iter()
			.find(|extension| extension.name == AS_COINAGE)
			.ok_or("missing AsCoinage extension")?;
		let types = self.subxt.types();
		let explicit = types.resolve(extension.id).ok_or("invalid AsCoinage type")?;
		let option = match &explicit.type_def {
			TypeDef::Composite(wrapper) => {
				let [field] = wrapper.fields.as_slice() else {
					return Err("invalid AsCoinage wrapper fields".into());
				};
				types.resolve(field.ty.id).ok_or("invalid AsCoinage option type")?
			},
			_ => explicit,
		};
		let TypeDef::Variant(option) = &option.type_def else {
			return Err("invalid AsCoinage option shape".into());
		};
		let some = option
			.variants
			.iter()
			.find(|variant| variant.name == "Some")
			.ok_or("missing AsCoinage.Some")?;
		let [field] = some.fields.as_slice() else {
			return Err("invalid AsCoinage.Some fields".into());
		};
		let info = types.resolve(field.ty.id).ok_or("invalid AsCoinage authorization type")?;
		let TypeDef::Variant(info) = &info.type_def else {
			return Err("invalid AsCoinage authorization enum".into());
		};
		info.variants
			.iter()
			.find(|variant| variant.name == name)
			.ok_or_else(|| format!("missing AsCoinage.{name}"))
	}

	/// Keep the runtime and mortality anchored to the same finalized snapshot
	/// used to validate the inputs, rather than reading a newer head at signing.
	fn extrinsics(&self, service: &ExtrinsicService) -> ExtrinsicService {
		service.with_runtime_at(
			self.genesis_hash.into(),
			RuntimeAtBirth {
				birth: HashAndNumber { hash: self.at.into(), number: self.number },
				spec_version: self.spec_version,
				transaction_version: self.transaction_version,
				metadata: self.subxt.clone(),
			},
		)
	}

	fn build<'a>(
		&self,
		service: &'a ExtrinsicService,
		call: &'a [u8],
		nonce: u32,
	) -> Result<ExtrinsicBuilder<'a>, String> {
		let info = self.validate_call(call)?;
		let pipeline = self
			.subxt
			.extrinsic_extension_info(Some(self.metadata.extension_version()))
			.map_err(|_| "invalid extension pipeline")?;
		if !pipeline.extension_ids.iter().any(|extension| extension.name == AS_COINAGE) ||
			!pipeline
				.extension_ids
				.iter()
				.any(|extension| extension.name == "VerifyMultiSignature")
		{
			return Err("Coinage authorization extensions missing".into());
		}
		Ok(service
			.build_mortal_extrinsic(self.genesis_hash.into())
			.with_extrinsic_version(ExtrinsicVersion::V5 {
				extension_version: self.metadata.extension_version(),
			})
			.with_call_args(&info.pallet_name, &info.call_name, CallArgs(&call[2..]))
			.with_nonce(u64::from(nonce))
			.with_mortality_period(self.period))
	}

	pub async fn signed(
		&self,
		service: &ExtrinsicService,
		keypair: &schnorrkel::Keypair,
		call: &[u8],
		nonce: u32,
	) -> Result<EncodedExtrinsic, String> {
		let service = self.extrinsics(service);
		let signer = Sr25519Signer::from_keypair(keypair);
		let extrinsic = self
			.build(&service, call, nonce)?
			.with_signer(&signer)
			.with_extension(CoinageAuthorization(std::cell::RefCell::new(
				|_: &InheritedImplication| self.as_coin(),
			)))
			.build()
			.await
			.map_err(|error| format!("Coinage transaction cannot be signed: {error}"))?;
		Ok(extrinsic.extrinsic)
	}

	/// The shared pipeline supplies the exact inherited implication to the
	/// proof provider, then validates and assembles its authorization bytes.
	pub async fn unsigned(
		&self,
		service: &ExtrinsicService,
		call: &[u8],
		mut prove: impl FnMut(&InheritedImplication) -> Result<truapi_coinage::UnloadTokenProof, String>
		+ Send,
	) -> Result<EncodedExtrinsic, String> {
		let service = self.extrinsics(service);
		let extrinsic = self
			.build(&service, call, 0)?
			.with_extension(CoinageAuthorization(std::cell::RefCell::new(
				|implication: &InheritedImplication| {
					let proof = prove(implication)?;
					let name = match proof.person_origin {
						truapi_coinage::PersonOriginKind::Full => "AsUnloadTokenPeople",
						truapi_coinage::PersonOriginKind::Lite => "AsUnloadTokenLitePeople",
					};
					let variant = self.authorization_variant(name)?.index;
					let mut extra = proof.as_coinage_extension().encode();
					if extra.len() < 2 {
						return Err("invalid Coinage unload proof encoding".into());
					}
					extra[1] = variant;
					Ok(extra)
				},
			)))
			.build()
			.await
			.map_err(|error| format!("Coinage unload transaction cannot be built: {error}"))?;
		Ok(extrinsic.extrinsic)
	}

	fn validate_call(&self, call: &[u8]) -> Result<ExtrinsicCallInfo<'_, u32>, String> {
		use subxt::ext::scale_decode::visitor::{IgnoreVisitor, decode_with_visitor};
		let [pallet, index, ..] = call else {
			return Err("Coinage call is truncated".into());
		};
		let info = self
			.subxt
			.extrinsic_call_info_by_index(*pallet, *index)
			.map_err(|_| "Coinage call metadata missing")?;
		if info.pallet_name != "Coinage" {
			return Err("Coinage adapter cannot submit another pallet".into());
		}
		let mut bytes = &call[2..];
		for argument in &info.args {
			decode_with_visitor(&mut bytes, argument.id, self.subxt.types(), IgnoreVisitor::new())
				.map_err(|_| "Coinage call shape changed")?;
		}
		if !bytes.is_empty() {
			return Err("Coinage call has trailing arguments".into());
		}
		Ok(info)
	}
}

/// Coinage's proof types already define their canonical SCALE bytes. The
/// pipeline checks these against the runtime's explicit type before assembly.
struct AuthorizationBytes(Vec<u8>);

impl EncodeAsType for AuthorizationBytes {
	fn encode_as_type_to<R: TypeResolver>(
		&self,
		_type_id: R::TypeId,
		_types: &R,
		out: &mut Vec<u8>,
	) -> Result<(), EncodeError> {
		out.extend_from_slice(&self.0);
		Ok(())
	}
}

struct CoinageAuthorization<F>(std::cell::RefCell<F>);

impl<F> TransactionExtension for CoinageAuthorization<F>
where
	F: FnMut(&InheritedImplication) -> Result<Vec<u8>, String> + Send,
{
	type Implicit = ();
	type Explicit = AuthorizationBytes;

	fn name(&self) -> &str {
		AS_COINAGE
	}

	fn implicit(&self) {}

	fn explicit(
		&self,
		implication: &InheritedImplication,
	) -> Result<AuthorizationBytes, ExtensionError> {
		(self.0.borrow_mut())(implication)
			.map(AuthorizationBytes)
			.map_err(|error| ExtensionError::new(std::io::Error::other(error)))
	}
}

/// Calls are validated against the pinned metadata before entering the
/// builder; preserve their reviewed argument bytes exactly.
struct CallArgs<'a>(&'a [u8]);

impl EncodeAsFields for CallArgs<'_> {
	fn encode_as_fields_to<R: TypeResolver>(
		&self,
		_fields: &mut dyn FieldIter<'_, R::TypeId>,
		_types: &R,
		out: &mut Vec<u8>,
	) -> Result<(), EncodeError> {
		out.extend_from_slice(self.0);
		Ok(())
	}
}

pub(super) fn denomination_value(
	context: &DenominationBreakdownContext,
	exponent: i16,
) -> Result<u128, String> {
	if exponent < context.min_exponent || exponent > context.max_exponent {
		return Err("denomination outside runtime range".into());
	}
	let value = if exponent >= 0 {
		let factor = 1u128.checked_shl(exponent as u32).ok_or("denomination overflow")?;
		context.asset_unit.checked_mul(factor).ok_or("denomination overflow")?
	} else {
		context
			.asset_unit
			.checked_shr(u32::from(exponent.unsigned_abs()))
			.ok_or("invalid denomination shift")?
	};
	if value == 0 {
		return Err("zero denomination".into());
	}
	Ok(value)
}

pub(super) fn constant<T: Decode>(
	metadata: &Metadata,
	pallet: &str,
	name: &str,
) -> Result<T, String> {
	decode_exact(
		metadata
			.constant(pallet, name)
			.ok_or("required Coinage runtime constant missing")?,
	)
}

pub(super) fn decode_exact<T: Decode>(bytes: &[u8]) -> Result<T, String> {
	let mut input = bytes;
	let value = T::decode(&mut input).map_err(|_| "invalid chain SCALE value")?;
	if !input.is_empty() {
		return Err("chain SCALE value has trailing bytes".into());
	}
	Ok(value)
}

pub(super) fn hex0x(bytes: &[u8]) -> String {
	format!("0x{}", hex::encode(bytes))
}
pub(super) fn parse_hash(value: &str) -> Result<[u8; 32], String> {
	hex::decode(value.strip_prefix("0x").ok_or("block hash lacks hex prefix")?)
		.map_err(|_| "invalid block hash hex")?
		.try_into()
		.map_err(|_| "invalid block hash width".into())
}

/// Resolve storage hashing from the same finalized metadata used for the proof.
pub(super) struct StorageLayouts {
	metadata: subxt::metadata::ArcMetadata,
	pub instance_id: Option<u32>,
}

impl StorageLayouts {
	fn new(
		metadata: subxt::metadata::ArcMetadata,
		configured_instance: Option<u32>,
	) -> Result<Self, String> {
		let instances = metadata
			.pallet_by_name("Coinage")
			.and_then(|pallet| pallet.storage())
			.and_then(|storage| storage.entry_by_name("Instances"))
			.is_some();
		let instance_id = if instances {
			Some(
				configured_instance
					.ok_or("Host must configure the Coinage asset instance for this runtime")?,
			)
		} else {
			None
		};
		let layouts = Self { metadata, instance_id };
		layouts.validate_coinage_keys()?;
		Ok(layouts)
	}

	fn entry(
		&self,
		pallet: &str,
		entry: &str,
	) -> Result<(&str, &subxt::metadata::StorageEntryMetadata), String> {
		let storage = self
			.metadata
			.pallet_by_name(pallet)
			.and_then(|pallet| pallet.storage())
			.ok_or_else(|| format!("missing storage metadata for {pallet}"))?;
		let definition = storage
			.entry_by_name(entry)
			.ok_or_else(|| format!("missing storage metadata for {pallet}.{entry}"))?;
		Ok((storage.prefix(), definition))
	}

	fn validate_coinage_keys(&self) -> Result<(), String> {
		use truapi_coinage::CoinageStorageKey as Key;
		let owner = [7; 32];
		for key in [
			Key::Coin(owner),
			Key::Recycler(owner),
			Key::RecyclerAlias { exponent: 0, ring_index: 1, alias: owner },
			Key::Member { exponent: 0, member: owner },
			Key::Root { exponent: 0, ring_index: 1 },
			Key::RingKeysStatus { exponent: 0, ring_index: 1 },
		] {
			self.coinage_key(&key)?;
		}
		Ok(())
	}

	pub(super) fn collection(&self, exponent: i16) -> [u8; 32] {
		truapi_coinage::pallet::recycler_collection_identifier(self.instance_id, exponent)
	}

	/// Encode semantic queries against metadata from the queried block, including
	/// historical recovery probes. Never infer a spent coin from an obsolete key.
	pub(super) fn coinage_key(
		&self,
		key: &truapi_coinage::CoinageStorageKey,
	) -> Result<Vec<u8>, String> {
		use truapi_coinage::CoinageStorageKey as Key;
		match key {
			Key::Coin(owner) => self.typed_key("Coinage", "CoinsByOwner", &[owner]),
			Key::Recycler(member) => {
				self.typed_key("Coinage", "RecyclersCoinToRecycler", &[member])
			},
			Key::RecyclerAlias { exponent, ring_index, alias } => match self.instance_id {
				Some(instance) => self.typed_key(
					"Coinage",
					"RecyclerAliasStates",
					&[
						&instance.to_le_bytes(),
						&exponent.to_le_bytes(),
						&ring_index.to_le_bytes(),
						alias,
					],
				),
				None => self.typed_key(
					"Coinage",
					"RecyclerAliasStates",
					&[&exponent.to_le_bytes(), &ring_index.to_le_bytes(), alias],
				),
			},
			Key::Member { exponent, member } => {
				self.typed_key("Members", "Members", &[&self.collection(*exponent), member])
			},
			Key::Root { exponent, ring_index } => self.typed_key(
				"Members",
				"Root",
				&[&self.collection(*exponent), &ring_index.to_le_bytes()],
			),
			Key::RingKeysStatus { exponent, ring_index } => self.typed_key(
				"Members",
				"RingKeysStatus",
				&[&self.collection(*exponent), &ring_index.to_le_bytes()],
			),
		}
	}

	fn typed_key(
		&self,
		pallet: &str,
		entry: &str,
		components: &[&[u8]],
	) -> Result<Vec<u8>, String> {
		let (_, definition) = self.entry(pallet, entry)?;
		if components.len() != definition.keys().len() {
			return Err(format!("storage key arity changed for {pallet}.{entry}"));
		}
		self.encode(pallet, entry, components.iter().copied())
	}

	fn encode<'a>(
		&self,
		pallet: &str,
		entry: &str,
		components: impl ExactSizeIterator<Item = &'a [u8]>,
	) -> Result<Vec<u8>, String> {
		use subxt::ext::{
			frame_decode::storage::StorageHasher,
			scale_decode::visitor::{IgnoreVisitor, decode_with_visitor},
		};
		let (prefix, definition) = self.entry(pallet, entry)?;
		if components.len() > definition.keys().len() {
			return Err("too many storage key components".into());
		}
		let mut output = sp_crypto_hashing::twox_128(prefix.as_bytes()).to_vec();
		output.extend_from_slice(&sp_crypto_hashing::twox_128(entry.as_bytes()));
		for (info, key) in definition.keys().zip(components) {
			let mut remaining = key;
			decode_with_visitor(
				&mut remaining,
				info.key_id,
				self.metadata.types(),
				IgnoreVisitor::new(),
			)
			.map_err(|_| format!("storage key type changed for {pallet}.{entry}"))?;
			if !remaining.is_empty() {
				return Err(format!("storage key has trailing bytes for {pallet}.{entry}"));
			}
			match info.hasher {
				StorageHasher::Identity => output.extend_from_slice(key),
				StorageHasher::Blake2_128Concat => {
					output.extend_from_slice(&sp_crypto_hashing::blake2_128(key));
					output.extend_from_slice(key);
				},
				StorageHasher::Twox64Concat => {
					output.extend_from_slice(&sp_crypto_hashing::twox_64(key));
					output.extend_from_slice(key);
				},
				StorageHasher::Blake2_128 => {
					output.extend_from_slice(&sp_crypto_hashing::blake2_128(key))
				},
				StorageHasher::Blake2_256 => {
					output.extend_from_slice(&sp_crypto_hashing::blake2_256(key))
				},
				StorageHasher::Twox128 => {
					output.extend_from_slice(&sp_crypto_hashing::twox_128(key))
				},
				StorageHasher::Twox256 => {
					output.extend_from_slice(&sp_crypto_hashing::twox_256(key))
				},
			}
		}
		Ok(output)
	}

	fn decode_value<T: scale_decode::DecodeAsType>(
		&self,
		pallet: &str,
		entry: &str,
		bytes: &[u8],
	) -> Result<T, String> {
		let (_, definition) = self.entry(pallet, entry)?;
		let mut remaining = bytes;
		let decoded =
			T::decode_as_type(&mut remaining, definition.value_ty(), self.metadata.types())
				.map_err(|_| format!("invalid storage value for {pallet}.{entry}"))?;
		if !remaining.is_empty() {
			return Err(format!("storage value has trailing bytes for {pallet}.{entry}"));
		}
		Ok(decoded)
	}

	/// Only the selected asset can enter the main-purse engine. The canonical
	/// engine row deliberately contains no guest-selectable asset or purse.
	pub(super) fn normalize_value(
		&self,
		key: &truapi_coinage::CoinageStorageKey,
		bytes: &mut Vec<u8>,
	) -> Result<(), String> {
		use truapi_coinage::CoinageStorageKey as Key;
		match key {
			Key::Coin(_) => {
				#[derive(scale_decode::DecodeAsType)]
				struct LegacyCoin {
					value: i8,
					age: u16,
				}
				#[derive(scale_decode::DecodeAsType)]
				struct InstanceCoin {
					instance_id: u32,
					value: i8,
					age: u16,
				}
				let coin = match self.instance_id {
					Some(expected) => {
						let coin: InstanceCoin =
							self.decode_value("Coinage", "CoinsByOwner", bytes)?;
						if coin.instance_id != expected {
							return Err("coin belongs to another Coinage asset instance".into());
						}
						LegacyCoin { value: coin.value, age: coin.age }
					},
					None => self.decode_value("Coinage", "CoinsByOwner", bytes)?,
				};
				bytes.clear();
				(coin.value, coin.age).encode_to(bytes);
			},
			Key::Recycler(_) => {
				let exponent = match self.instance_id {
					Some(expected) => {
						let (instance, exponent): (u32, i8) =
							self.decode_value("Coinage", "RecyclersCoinToRecycler", bytes)?;
						if instance != expected {
							return Err("voucher belongs to another Coinage asset instance".into());
						}
						exponent
					},
					None => self.decode_value::<i8>("Coinage", "RecyclersCoinToRecycler", bytes)?,
				};
				bytes.clear();
				exponent.encode_to(bytes);
			},
			_ => {
				use subxt::ext::scale_decode::visitor::{IgnoreVisitor, decode_with_visitor};
				let (pallet, entry) = match key {
					Key::RecyclerAlias { .. } => ("Coinage", "RecyclerAliasStates"),
					Key::Member { .. } => ("Members", "Members"),
					Key::Root { .. } => ("Members", "Root"),
					Key::RingKeysStatus { .. } => ("Members", "RingKeysStatus"),
					_ => unreachable!(),
				};
				let (_, definition) = self.entry(pallet, entry)?;
				let mut remaining = bytes.as_slice();
				decode_with_visitor(
					&mut remaining,
					definition.value_ty(),
					self.metadata.types(),
					IgnoreVisitor::new(),
				)
				.map_err(|_| format!("invalid storage value for {pallet}.{entry}"))?;
				if !remaining.is_empty() {
					return Err("Coinage storage value has trailing bytes".into());
				}
			},
		}
		Ok(())
	}
}

pub(super) fn storage_key(
	layouts: &StorageLayouts,
	pallet_name: &str,
	entry_name: &str,
	keys: &[Vec<u8>],
) -> Result<Vec<u8>, String> {
	layouts.encode(pallet_name, entry_name, keys.iter().map(Vec::as_slice))
}
