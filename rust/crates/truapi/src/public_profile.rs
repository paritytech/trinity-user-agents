//! Public profile records resolved from dotNS.

#[cfg(not(target_arch = "wasm32"))]
mod dao;
mod record;

use std::sync::Arc;

use alloy_primitives::B256;
use alloy_sol_types::SolCall;
use tracing::{instrument, warn};

pub use record::{ProfileAvatar, ProfileEntry, ProfileRecord};

#[cfg(not(target_arch = "wasm32"))]
use crate::{store::DbError, unix_time::current_unix_secs};

use crate::{
	dotns_views::{decode_view, network_tld_from_view, protocol_registry, read_view, tld_node},
	host_logic::dotns_gateway::{
		ContentResolver, DotnsError, DotnsRegistry, DotnsTransport, ProtocolRegistry,
		is_dotted_lite_username, namehash_under,
	},
	platform::is_dns_label,
	runtime::{DotnsLookup, DotnsLookupKind, services::RuntimeServices},
};

/// Text record a username publishes its profile at.
const PROFILE_RECORD_KEY: &str = "profile";
/// How long a resolved record is served before the chain is read again.
#[cfg(not(target_arch = "wasm32"))]
const RECORD_TTL_SECS: u64 = 60 * 60;

/// The understood metadata in a dotNS public profile.
///
/// Image bytes are loaded separately by CID. Missing or malformed records have no avatar.
#[derive(Debug, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PublicProfile {
	/// The first understood avatar, if the record names one.
	pub avatar: Option<ProfileAvatar>,
}

/// Why a public profile could not be resolved.
#[derive(Debug, thiserror::Error, derive_more::From)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Error))]
#[cfg_attr(not(target_arch = "wasm32"), uniffi(flat_error))]
pub enum PublicProfileError {
	/// Not a username the core resolves.
	#[error("not a dotNS username")]
	InvalidUsername,
	/// The host configured no Asset Hub chain.
	#[error("no Asset Hub configured")]
	AssetHubNotConfigured,
	/// The chain was out of reach, or the lookup failed.
	#[error("dotNS lookup failed: {}", .0.reason())]
	#[from(DotnsError)]
	Dotns(#[source] Box<DotnsError>),
	/// The core database could not be read or written.
	#[cfg(not(target_arch = "wasm32"))]
	#[error("core database unavailable: {0}")]
	#[from(DbError)]
	Storage(#[source] DbError),
}

/// The node and contracts that hold a username's public profile.
///
/// Resolve from the deployment before reading, checking ownership or preparing a write.
#[derive(Debug, PartialEq)]
pub struct ProfileRecordLocation {
	/// Username namehash under the deployment's TLD.
	node: [u8; 32],
	/// Forward registry that determines the node's owner and delegates.
	registry: [u8; 20],
	/// Content resolver registered by the deployment.
	pub resolver: [u8; 20],
}

impl ProfileRecordLocation {
	/// Encodes the call to publish this node's profile record.
	///
	/// Returns `None` when the SCALE record exceeds 16 KiB before base64 encoding.
	pub fn set_record_call(&self, record: &ProfileRecord) -> Option<Vec<u8>> {
		let call = ContentResolver::setTextCall {
			node: self.node.into(),
			key: PROFILE_RECORD_KEY.to_string(),
			value: record.to_text()?,
		};
		Some(call.abi_encode())
	}

	/// Finds the profile's content resolver without changing the node's resolver.
	///
	/// Returns `None` when the deployment lacks the required contracts. Invalid usernames
	/// and failed chain reads return errors.
	pub async fn resolve<T: DotnsTransport + ?Sized>(
		transport: &mut T,
		username: &str,
	) -> Result<Option<Self>, PublicProfileError> {
		validate_username(username)?;
		let Some(protocol_registry) = protocol_registry(transport).await? else {
			return Ok(None);
		};
		let results = transport
			.views(
				&protocol_registry,
				vec![
					ProtocolRegistry::tldCall {}.abi_encode(),
					ProtocolRegistry::getCall { key: B256::right_padding_from(b"registry") }
						.abi_encode(),
					ProtocolRegistry::getCall { key: B256::right_padding_from(b"contentResolver") }
						.abi_encode(),
				],
			)
			.await;
		let [tld, registry, resolver] = <[_; 3]>::try_from(results).expect("one result per view");
		let component = |name: &str, output| {
			decode_view::<ProtocolRegistry::getCall>(output)
				.map(|address| address.into_array())
				.map_err(|source| DotnsError::ProtocolComponent { name: name.to_string(), source })
		};
		let registry = component("registry", registry)?;
		let resolver = component("contentResolver", resolver)?;
		if registry == [0; 20] || resolver == [0; 20] {
			return Ok(None);
		}
		let tld = network_tld_from_view(transport, tld, &registry).await?;
		Ok(Some(Self { node: username_node(&tld, username), registry, resolver }))
	}

	/// Reads the raw text so a writer can reject malformed records without losing them.
	///
	/// Empty text returns `None`; chain and decoding failures return errors.
	pub async fn read<T: DotnsTransport + ?Sized>(
		&self,
		transport: &mut T,
	) -> Result<Option<String>, DotnsError> {
		let text = read_view(
			transport,
			&self.resolver,
			ContentResolver::textCall {
				node: self.node.into(),
				key: PROFILE_RECORD_KEY.to_string(),
			},
		)
		.await
		.map_err(DotnsError::TextRecord)?;
		Ok((!text.is_empty()).then_some(text))
	}

	/// Returns the registered owner of the profile's name.
	///
	/// Writers compare this address with their signer before spending its allowance.
	/// Chain and decoding failures return an owner lookup error.
	pub async fn owner<T: DotnsTransport + ?Sized>(
		&self,
		transport: &mut T,
	) -> Result<[u8; 20], DotnsError> {
		let owner = read_view(
			transport,
			&self.registry,
			DotnsRegistry::ownerCall { node: self.node.into() },
		)
		.await
		.map_err(DotnsError::Owner)?;
		Ok(owner.into_array())
	}
}

/// Resolves public profile records through the host's shared services.
///
/// Native hosts persist records in the core database. Browser reads are uncached.
pub struct PublicProfiles {
	services: Arc<RuntimeServices>,
}

impl PublicProfiles {
	/// Uses the host's shared chain and native storage.
	pub fn new(services: Arc<RuntimeServices>) -> Self {
		Self { services }
	}

	/// Resolves a profile using the native record cache.
	///
	/// Records expire after one hour; `force` bypasses the cache. Missing or malformed
	/// records are empty. Invalid usernames, lookup failures and storage failures return errors.
	#[cfg(not(target_arch = "wasm32"))]
	#[instrument(skip_all, fields(runtime.method = "public_profile.resolve"))]
	pub async fn resolve(
		&self,
		username: &str,
		force: bool,
	) -> Result<PublicProfile, PublicProfileError> {
		validate_username(username)?;
		let db = self.services.core_db()?;
		let now = current_unix_secs();
		if !force {
			let cached_username = username.to_owned();
			let cached =
				db.read(move |conn| dao::fresh_record(conn, &cached_username, now)).await?;
			if let Some(record) = cached {
				return Ok(PublicProfile { avatar: record.avatar() });
			}
		}

		let record = self.read_record(username).await?;
		let (stored_username, stored_record) = (username.to_owned(), record.clone());
		db.write(move |tx| {
			dao::store_record(tx, &stored_username, &stored_record, now, now + RECORD_TTL_SECS)
		})
		.await?;
		Ok(PublicProfile { avatar: record.avatar() })
	}

	/// Resolves a profile directly from the chain.
	///
	/// Missing or malformed records are empty. Invalid usernames and lookup failures
	/// return errors. The refresh argument has no effect because browsers do not cache records.
	#[cfg(target_arch = "wasm32")]
	#[instrument(skip_all, fields(runtime.method = "public_profile.resolve"))]
	pub async fn resolve(
		&self,
		username: &str,
		_force: bool,
	) -> Result<PublicProfile, PublicProfileError> {
		validate_username(username)?;
		let record = self.read_record(username).await?;
		Ok(PublicProfile { avatar: record.avatar() })
	}

	async fn read_record(&self, username: &str) -> Result<ProfileRecord, PublicProfileError> {
		let genesis = self
			.services
			.asset_hub_chain_genesis_hash()
			.ok_or(PublicProfileError::AssetHubNotConfigured)?;
		let mut lookup = DotnsLookup::pinned_to_best_block(
			&self.services.chain,
			genesis,
			DotnsLookupKind::PublicProfile,
		)
		.await?;
		Self::read_record_from(&mut lookup, username).await
	}

	async fn read_record_from<T: DotnsTransport + ?Sized>(
		transport: &mut T,
		username: &str,
	) -> Result<ProfileRecord, PublicProfileError> {
		let Some(location) = ProfileRecordLocation::resolve(transport, username).await? else {
			return Ok(ProfileRecord::default());
		};
		let text = location.read(transport).await?;
		let record = text.as_deref().map(ProfileRecord::from_text);
		if let Some(None) = record {
			warn!("the profile record does not decode; treating it as none");
		}
		Ok(record.flatten().unwrap_or_default())
	}
}

fn validate_username(username: &str) -> Result<(), PublicProfileError> {
	if is_dns_label(username) || is_dotted_lite_username(username) {
		Ok(())
	} else {
		Err(PublicProfileError::InvalidUsername)
	}
}

fn username_node(tld: &str, username: &str) -> [u8; 32] {
	username
		.rsplit('.')
		.fold(tld_node(tld), |parent, part| namehash_under(&parent, part))
}

#[cfg(test)]
mod tests;
