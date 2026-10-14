//! Warm-start storage: the provider owns when a blob is read and written, the
//! host owns where it lives.
//!
//! A light client that starts from a stored finalized-database blob resumes
//! from that state instead of warp syncing from the chain-spec checkpoint. The
//! blob itself is opaque, so the only thing a host has to supply is somewhere
//! to keep it: an implementation of [`StorageClient`] over whatever storage the
//! platform already offers. The crate stores nothing itself, so it never
//! competes with the host for the same quota and never decides on its behalf
//! whether the bytes are backed up or encrypted.
//!
//! Reads and writes are explicit, through
//! [`load_database`](crate::EmbeddedChainProvider::load_database) and
//! [`save_database`](crate::EmbeddedChainProvider::save_database), rather than
//! hidden inside `connect`. Connecting is a blocking call on the native
//! bindings. Awaiting a foreign callback underneath it would deadlock a host
//! whose client runs on the main thread.

/// Failure reported by a [`StorageClient`] implementation.
///
/// An enum with one variant rather than a struct because the native bindings
/// export this type, and uniffi errors must be enums.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[cfg_attr(all(feature = "uniffi", not(target_arch = "wasm32")), derive(uniffi::Error))]
pub enum StorageClientError {
	/// The store could not read or write the blob.
	#[display("warm store: {reason}")]
	Failed {
		/// What went wrong, for logs and error reports.
		reason: String,
	},
}

impl StorageClientError {
	/// Build an error from anything printable.
	pub fn new(reason: impl core::fmt::Display) -> Self {
		Self::Failed { reason: reason.to_string() }
	}

	/// What went wrong.
	pub fn reason(&self) -> &str {
		let Self::Failed { reason } = self;
		reason
	}
}

impl std::error::Error for StorageClientError {}

/// Where warm-start blobs are kept between runs.
///
/// A client that cannot answer must return `Err`, never `Ok(None)`: an empty
/// read is taken as nothing being stored for that chain yet, and lets a later
/// [`save_database`](crate::EmbeddedChainProvider::save_database) overwrite good state.
///
/// A loaded blob is trusted input: it goes to the light client as the finalized
/// state to resume from, so whatever can write to the store can steer the
/// view the client has of the chain. Keep storage no more writable than the source
/// the chain specification itself came from.
#[cfg_attr(all(feature = "uniffi", not(target_arch = "wasm32")), uniffi::export(with_foreign))]
#[async_trait::async_trait]
pub trait StorageClient: Send + Sync {
	/// Read the blob stored for `genesis_hash`, if any.
	async fn load(&self, genesis_hash: [u8; 32]) -> Result<Option<String>, StorageClientError>;

	/// Replace the blob stored for `genesis_hash`.
	async fn save(&self, genesis_hash: [u8; 32], blob: String) -> Result<(), StorageClientError>;
}

/// Whether `blob` carries the runtime code.
///
/// A blob without it still resumes from finalized state, because the chain
/// information decides that on its own. What it costs is a runtime download on
/// the next start, which is why a blob that has the code must never be replaced
/// by one that does not.
pub fn carries_runtime_code(blob: &str) -> bool {
	// smoldot serialises the runtime code as `runtimeCode` and omits the key
	// when it has none, and its shrink ladder drops that field first when a
	// snapshot is over the size cap. A blob without it still skips the warp
	// sync, since the chain information in the database is chosen on finalized block
	// number alone, but it costs a runtime download on the next start.
	blob.contains("\"runtimeCode\":")
}

/// Whether `blob` is worth storing, given whether what is already stored
/// carries the runtime code. `None` means nothing is stored.
///
/// A snapshot taken early, or shrunk to fit the size cap, can carry chain
/// information without the runtime code. That is still usable, so it is worth
/// keeping when nothing is stored, but it must not replace a blob that has the
/// code, which would trade a resumed start for a runtime download every run.
///
/// The quality of the stored blob is passed in rather than the blob itself, so
/// a caller that already knows what it wrote does not read megabytes back to
/// find out.
pub fn is_worth_storing(blob: &str, stored_has_runtime_code: Option<bool>) -> bool {
	if !carries_chain_information(blob) {
		return false;
	}
	match stored_has_runtime_code {
		Some(true) => carries_runtime_code(blob),
		Some(false) | None => true,
	}
}

/// Whether `blob` carries the finalized chain information.
///
/// smoldot answers `chainHead_unstable_finalizedDatabase` even for a chain that
/// has finalized nothing yet, and the blob it returns then decodes to a database
/// with no chain information, which it discards on the next run. Storing one
/// over a good blob trades a resumed start for a cold one.
pub fn carries_chain_information(blob: &str) -> bool {
	// A substring test rather than a parse: this runs on every snapshot against
	// a blob of up to 8 MB, and the encoder in smoldot omits the key entirely when
	// there is no chain information and writes it as an object when there is
	// (`skip_serializing_if` on `SerdeDatabase::chain`). The truncation
	// sentinels it can return instead, `"<too-large>"` and the empty string,
	// carry no key at all.
	blob.contains("\"chain\":{")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_blob_without_chain_information_is_not_worth_storing() {
		assert!(!carries_chain_information(r#"{"chain":null}"#));
		assert!(!carries_chain_information("{}"));
		assert!(!carries_chain_information("not json"));
		assert!(!carries_chain_information("<too-large>"));
		assert!(!carries_chain_information(""));
		assert!(carries_chain_information(
			r#"{"chain":{"finalized_block_header":"0x00"},"genesisHash":"0x01"}"#
		));
	}

	#[test]
	fn a_blob_without_runtime_code_never_replaces_one_that_has_it() {
		const WITH_CODE: &str = r#"{"chain":{"a":1},"runtimeCode":"AAAA"}"#;
		const WITHOUT_CODE: &str = r#"{"chain":{"a":1}}"#;

		assert!(
			is_worth_storing(WITHOUT_CODE, None),
			"a blob with no runtime code still beats nothing stored"
		);
		assert!(
			is_worth_storing(WITH_CODE, Some(false)),
			"gaining the runtime code is an improvement"
		);
		assert!(
			!is_worth_storing(WITHOUT_CODE, Some(true)),
			"losing the runtime code would cost a runtime download every run"
		);
		assert!(
			is_worth_storing(WITH_CODE, Some(true)),
			"a fresher blob of the same quality is still worth storing"
		);
		assert!(
			!is_worth_storing(r#"{"genesisHash":"0x01"}"#, None),
			"a blob with no chain information is worth nothing"
		);
	}
}
