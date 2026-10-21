//! Encoding the keys and decoding the values of a subxt storage address
//! against runtime metadata.

use derive_more::Display;
use subxt::{
	Metadata,
	ext::{
		frame_decode::storage::{
			IntoEncodableValues, StorageInfo, StorageTypeInfo, decode_storage_value_with_info,
			encode_storage_key_with_info,
		},
		scale_decode::IntoVisitor,
	},
	storage::Address,
};

use super::StorageWatchError;
use crate::chain::HashAndNumber;

/// A storage entry by pallet and entry name, shown as `Pallet.Entry`.
#[derive(Clone, Debug, PartialEq, Eq, Display)]
#[display("{pallet}.{entry}")]
pub struct StorageEntryName {
	/// Pallet that declares the entry.
	pub pallet: String,
	/// Name of the entry within the pallet.
	pub entry: String,
}

impl StorageEntryName {
	fn of_address(address: &impl Address) -> Self {
		Self { pallet: address.pallet_name().to_owned(), entry: address.entry_name().to_owned() }
	}
}

/// The full storage key of each of `keys`, under `address`.
pub fn encode_full_storage_keys<A>(
	address: &A,
	keys: &[A::KeyParts],
	metadata: &Metadata,
) -> Result<Vec<Vec<u8>>, StorageWatchError>
where
	A: Address,
	A::KeyParts: Clone,
{
	let info = storage_entry_type_info(address, metadata)?;
	keys.iter()
		.enumerate()
		.map(|(index, parts)| {
			encode_full_storage_key(address, index, parts.clone(), &info, metadata)
		})
		.collect()
}

/// `bytes` decoded as the value of `address`, or its default when absent.
/// `None` when the entry is absent and has no default.
pub fn decode_value_or_default<A: Address>(
	address: &A,
	bytes: Option<&[u8]>,
	metadata: &Metadata,
	at: HashAndNumber,
) -> Result<Option<A::Value>, StorageWatchError> {
	let info = storage_entry_type_info(address, metadata)?;
	let Some(bytes) = bytes.or(info.default_value.as_deref()) else { return Ok(None) };
	let decoded = decode_value_rejecting_leftover_bytes(address, bytes, &info, metadata, at)?;
	Ok(Some(decoded))
}

fn storage_entry_type_info<'metadata>(
	address: &impl Address,
	metadata: &'metadata Metadata,
) -> Result<StorageInfo<'metadata, u32>, StorageWatchError> {
	metadata
		.storage_info(address.pallet_name(), address.entry_name())
		.map_err(|error| StorageWatchError::Incompatible {
			entry: StorageEntryName::of_address(address),
			reason: error.to_string(),
		})
}

fn encode_full_storage_key<A: Address>(
	address: &A,
	index: usize,
	parts: A::KeyParts,
	info: &StorageInfo<'_, u32>,
	metadata: &Metadata,
) -> Result<Vec<u8>, StorageWatchError> {
	let key_error = |reason: String| StorageWatchError::KeyEncoding {
		entry: StorageEntryName::of_address(address),
		index,
		reason,
	};
	let given = parts.num_encodable_values();
	if given != info.keys.len() {
		return Err(key_error(format!("expected {} key parts, got {given}", info.keys.len())));
	}
	let (pallet, entry) = (address.pallet_name(), address.entry_name());
	encode_storage_key_with_info(pallet, entry, parts, info, metadata.types())
		.map_err(|error| key_error(error.to_string()))
}

fn decode_value_rejecting_leftover_bytes<A: Address>(
	address: &A,
	bytes: &[u8],
	info: &StorageInfo<'_, u32>,
	metadata: &Metadata,
	at: HashAndNumber,
) -> Result<A::Value, StorageWatchError> {
	let decode_error = |reason: String| StorageWatchError::Decode {
		entry: StorageEntryName::of_address(address),
		at,
		reason,
	};
	let cursor = &mut &*bytes;
	let visitor = A::Value::into_visitor();
	let value = decode_storage_value_with_info(cursor, info, metadata.types(), visitor)
		.map_err(|error| decode_error(error.to_string()))?;
	if !cursor.is_empty() {
		return Err(decode_error(format!("{} of {} bytes left over", cursor.len(), bytes.len())));
	}
	Ok(value)
}
