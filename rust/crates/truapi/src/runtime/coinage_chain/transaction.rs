// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer
// core/crates/brevity-ffi/src/{coinage_sender,coinage_transfer}.rs. Copyright the Brevity
// contributors. See truapi-coinage/NOTICE and LICENSE.

use crate::{
	chain::{
		ChainValues, ExtensionValueTypes, ExtrinsicVersion, HashAndNumber, InheritedImplication,
		Mortality, default_value_for_type, well_known_extension,
	},
	host_internal::extrinsic::{Sr25519Signer, build_signed_extrinsic_v5},
	runtime::statement_allowance::extension::Metadata,
};
use parity_scale_codec::{Compact, Decode, Encode};
use subxt::ext::frame_decode::extrinsics::ExtrinsicTypeInfo;
use truapi::latest::TxPayloadExtension;
use truapi_coinage::DenominationBreakdownContext;

pub(super) const AS_COINAGE: &str = "AsCoinage";

#[derive(Clone)]
pub(super) struct Snapshot {
	pub storage: std::sync::Arc<StorageLayouts>,
	pub metadata: std::sync::Arc<Metadata>,
	pub subxt: subxt::metadata::ArcMetadata,
	pub spec_version: u32,
	pub transaction_version: u32,
	pub genesis_hash: [u8; 32],
	pub extension_version: Option<u8>,
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
		extension_version: Option<u8>,
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
			spec_version,
			transaction_version,
			genesis_hash,
			extension_version,
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

	pub fn extensions(&self, nonce: u32) -> Result<Vec<TxPayloadExtension>, String> {
		if !self.subxt.extrinsic().supported_versions().contains(&5) {
			return Err("Coinage requires V5 extrinsic support".into());
		}
		let version = self.configured_extension_version()?;
		let info = self
			.subxt
			.extrinsic_extension_info(Some(version))
			.map_err(|_| "invalid extension pipeline")?;
		let chain = ChainValues {
			genesis: self.genesis_hash.into(),
			spec_version: self.spec_version,
			transaction_version: self.transaction_version,
			mortality: Mortality::new(
				HashAndNumber { hash: self.at.into(), number: self.number },
				self.period,
			)
			.map_err(|error| error.to_string())?,
			nonce: u64::from(nonce),
		};
		// Only resting values are prepared here. The actual signature and
		// Coinage proof are supplied after their inherited implication is known.
		let implication = InheritedImplication::new(
			Vec::new(),
			ExtrinsicVersion::V5 { extension_version: version },
		);
		let mut extensions = Vec::with_capacity(info.extension_ids.len());
		for definition in &info.extension_ids {
			let (extra, additional_signed) = if let Some(extension) =
				well_known_extension(&definition.name, &chain)
			{
				let values = extension
					.encode(
						&implication,
						ExtensionValueTypes {
							explicit: definition.id,
							implicit: definition.implicit_id,
						},
						self.subxt.types(),
					)
					.map_err(|error| error.to_string())?;
				(values.explicit, values.implicit)
			} else {
				let implicit = default_value_for_type(definition.implicit_id, self.subxt.types())
					.filter(Vec::is_empty)
					.ok_or("Coinage runtime requires an unrecognized extension implicit")?;
				// Canonical defaults only admit empty types and genuine
				// Option::None (including newtype wrappers): absent authority,
				// never a guessed zero for an unknown bool, enum or integer.
				let explicit = default_value_for_type(definition.id, self.subxt.types())
					.ok_or("Coinage runtime requires an unrecognized extension")?;
				(explicit, implicit)
			};
			extensions.push(TxPayloadExtension {
				id: definition.name.to_string(),
				extra,
				additional_signed,
			});
		}
		for required in [AS_COINAGE, "VerifyMultiSignature", "CheckMortality", "CheckNonce"] {
			if extensions.iter().filter(|extension| extension.id == required).count() != 1 {
				return Err("Coinage authorization extensions missing or repeated".into());
			}
		}
		self.validate_extensions(&extensions)?;
		Ok(extensions)
	}

	fn configured_extension_version(&self) -> Result<u8, String> {
		self.extension_version
			.ok_or_else(|| "Coinage requires a configured V5 pipeline".into())
	}

	pub fn as_coin(&self) -> Result<Vec<u8>, String> {
		let variant = self.coinage_origin("AsCoin")?;
		if !variant.fields.is_empty() {
			return Err("invalid AsCoinage.AsCoin fields".into());
		}
		Ok(vec![1, variant.index])
	}

	/// Resolve the proof variant in the configured pipeline, not whichever
	/// pipeline happens to have the largest version number.
	fn coinage_origin(
		&self,
		name: &str,
	) -> Result<&scale_info::Variant<scale_info::form::PortableForm>, String> {
		use scale_info::TypeDef;
		let info = self
			.subxt
			.extrinsic_extension_info(Some(self.configured_extension_version()?))
			.map_err(|_| "invalid extension pipeline")?;
		let mut id = info
			.extension_ids
			.iter()
			.find(|extension| extension.name == AS_COINAGE)
			.ok_or("missing AsCoinage extension")?
			.id;
		let types = self.subxt.types();
		for _ in 0..types.types.len() {
			let ty = types.resolve(id).ok_or("invalid AsCoinage type")?;
			match &ty.type_def {
				TypeDef::Composite(composite) if composite.fields.len() == 1 => {
					id = composite.fields[0].ty.id;
				},
				TypeDef::Variant(option)
					if ty.path.segments.last().is_some_and(|s| s == "Option") =>
				{
					if !option
						.variants
						.iter()
						.any(|v| v.name == "None" && v.index == 0 && v.fields.is_empty())
					{
						return Err("invalid AsCoinage option".into());
					}
					let some = option
						.variants
						.iter()
						.find(|v| v.name == "Some" && v.index == 1 && v.fields.len() == 1)
						.ok_or("invalid AsCoinage option")?;
					let inner =
						types.resolve(some.fields[0].ty.id).ok_or("invalid AsCoinage origin")?;
					let TypeDef::Variant(origins) = &inner.type_def else {
						return Err("invalid AsCoinage origin".into());
					};
					return origins
						.variants
						.iter()
						.find(|variant| variant.name == name)
						.ok_or_else(|| "missing AsCoinage origin variant".into());
				},
				_ => return Err("invalid AsCoinage authorization shape".into()),
			}
		}
		Err("recursive AsCoinage authorization shape".into())
	}

	pub fn signed(
		&self,
		keypair: &schnorrkel::Keypair,
		call: &[u8],
		nonce: u32,
	) -> Result<Vec<u8>, String> {
		self.validate_call(call)?;
		let mut extensions = self.extensions(nonce)?;
		replace(&mut extensions, AS_COINAGE, self.as_coin()?, Vec::new())?;
		self.validate_extensions(&extensions)?;
		extensions.retain(|extension| extension.id != "VerifyMultiSignature");
		build_signed_extrinsic_v5(
			&Sr25519Signer::from_keypair(keypair),
			self.genesis_hash,
			call,
			&extensions,
			self.configured_extension_version()?,
			self.subxt.clone(),
		)
		.map_err(|_| "Coinage transaction cannot be signed against runtime metadata".into())
	}

	pub fn implication(
		&self,
		call: &[u8],
		extensions: &[TxPayloadExtension],
	) -> Result<Vec<u8>, String> {
		self.validate_call(call)?;
		self.validate_extensions(extensions)?;
		let index = extensions
			.iter()
			.position(|extension| extension.id == AS_COINAGE)
			.ok_or("missing Coinage extension")?;
		let mut result = vec![self.configured_extension_version()?];
		result.extend_from_slice(call);
		for extension in &extensions[index + 1..] {
			result.extend_from_slice(&extension.extra);
		}
		for extension in &extensions[index + 1..] {
			result.extend_from_slice(&extension.additional_signed);
		}
		Ok(result)
	}

	pub fn unsigned(
		&self,
		call: &[u8],
		mut extensions: Vec<TxPayloadExtension>,
		proof: &truapi_coinage::UnloadTokenProof,
	) -> Result<Vec<u8>, String> {
		let mut extra = proof.as_coinage_extension().encode();
		let name = match proof.person_origin {
			truapi_coinage::PersonOriginKind::Full => "AsUnloadTokenPeople",
			truapi_coinage::PersonOriginKind::Lite => "AsUnloadTokenLitePeople",
		};
		let variant = self.coinage_origin(name)?.index;
		if extra.len() < 2 {
			return Err("invalid Coinage unload proof encoding".into());
		}
		extra[1] = variant;
		replace(&mut extensions, AS_COINAGE, extra, Vec::new())?;
		self.validate_call(call)?;
		self.validate_extensions(&extensions)?;
		let mut body = vec![0x45, self.configured_extension_version()?];
		for extension in &extensions {
			body.extend_from_slice(&extension.extra);
		}
		body.extend_from_slice(call);
		let mut output =
			Compact(u32::try_from(body.len()).map_err(|_| "Coinage transaction too large")?)
				.encode();
		output.extend_from_slice(&body);
		Ok(output)
	}

	fn validate_extensions(&self, extensions: &[TxPayloadExtension]) -> Result<(), String> {
		use subxt::ext::scale_decode::visitor::{IgnoreVisitor, decode_with_visitor};
		let info = self
			.subxt
			.extrinsic_extension_info(Some(self.configured_extension_version()?))
			.map_err(|_| "invalid extension pipeline")?;
		if info.extension_ids.len() != extensions.len() {
			return Err("extension pipeline mismatch".into());
		}
		for (definition, extension) in info.extension_ids.iter().zip(extensions) {
			if definition.name != extension.id {
				return Err("extension order mismatch".into());
			}
			let mut bytes = extension.extra.as_slice();
			decode_with_visitor(
				&mut bytes,
				definition.id,
				self.subxt.types(),
				IgnoreVisitor::new(),
			)
			.map_err(|_| "invalid Coinage extension encoding")?;
			if !bytes.is_empty() {
				return Err("Coinage extension has trailing bytes".into());
			}
			let mut implicit = extension.additional_signed.as_slice();
			decode_with_visitor(
				&mut implicit,
				definition.implicit_id,
				self.subxt.types(),
				IgnoreVisitor::new(),
			)
			.map_err(|_| "invalid Coinage extension implicit")?;
			if !implicit.is_empty() {
				return Err("Coinage extension implicit has trailing bytes".into());
			}
		}
		Ok(())
	}

	fn validate_call(&self, call: &[u8]) -> Result<(), String> {
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
		Ok(())
	}
}

pub(super) fn replace(
	extensions: &mut [TxPayloadExtension],
	name: &str,
	extra: Vec<u8>,
	additional_signed: Vec<u8>,
) -> Result<(), String> {
	let extension = extensions
		.iter_mut()
		.find(|extension| extension.id == name)
		.ok_or("required Coinage extension missing")?;
	extension.extra = extra;
	extension.additional_signed = additional_signed;
	Ok(())
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
