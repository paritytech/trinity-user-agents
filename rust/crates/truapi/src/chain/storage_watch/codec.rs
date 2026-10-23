//! Encoding the keys and decoding the values of a subxt storage address.
//!
//! Keys are encoded by subxt itself. Values are decoded here with frame-decode,
//! the same way subxt's `StorageValue::decode` does, because subxt does not let
//! callers build a `StorageValue` from bytes they read themselves.

use derive_more::Display;
use subxt::{
	Metadata,
	client::{ClientAtBlock, OfflineClientAtBlockT},
	config::Config,
	ext::{
		frame_decode::storage::{StorageInfo, StorageTypeInfo, decode_storage_value_with_info},
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

/// The full storage key of each of `keys`, under `address`, encoded by
/// subxt with the runtime of `client`.
pub fn encode_storage_keys<A, T, Client>(
	address: &A,
	keys: &[A::KeyParts],
	client: &ClientAtBlock<T, Client>,
) -> Result<Vec<Vec<u8>>, StorageWatchError>
where
	A: Address,
	A::KeyParts: Clone,
	T: Config,
	Client: OfflineClientAtBlockT<T>,
{
	let storage = client.storage();
	let entry = storage
		.entry(address)
		.map_err(|error| incompatible_entry(address, error.to_string()))?;
	keys.iter()
		.enumerate()
		.map(|(index, parts)| {
			entry.fetch_key(parts.clone()).map_err(|error| StorageWatchError::KeyEncoding {
				entry: StorageEntryName::of_address(address),
				index,
				reason: error.to_string(),
			})
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
	let info = entry_type_info(address, metadata)?;
	let Some(bytes) = bytes.or(info.default_value.as_deref()) else { return Ok(None) };
	let decoded = decode_value_strict(address, bytes, &info, metadata, at)?;
	Ok(Some(decoded))
}

fn entry_type_info<'metadata>(
	address: &impl Address,
	metadata: &'metadata Metadata,
) -> Result<StorageInfo<'metadata, u32>, StorageWatchError> {
	metadata
		.storage_info(address.pallet_name(), address.entry_name())
		.map_err(|error| incompatible_entry(address, error.to_string()))
}

/// The error for an `address` that the runtime metadata does not describe.
fn incompatible_entry(address: &impl Address, reason: String) -> StorageWatchError {
	StorageWatchError::Incompatible { entry: StorageEntryName::of_address(address), reason }
}

fn decode_value_strict<A: Address>(
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
