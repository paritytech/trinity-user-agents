//! Public profile metadata resolved from dotNS through [`PublicProfiles`].

#[cfg(not(target_arch = "wasm32"))]
mod dao;
mod record;

use std::sync::Arc;

use alloy_primitives::B256;
use alloy_sol_types::SolCall;
use futures::stream;
use tracing::warn;

pub use record::ProfileAvatar;
#[cfg(test)]
use record::ProfileEntry;
use record::ProfileRecord;

#[cfg(not(target_arch = "wasm32"))]
use crate::{store::DbError, unix_time::current_unix_secs};

use crate::{
	Subscription,
	dotns_views::{decode_view, network_tld_from_view, protocol_registry, read_view, tld_node},
	host_logic::dotns_gateway::{
		ContentResolver, DotnsError, DotnsTransport, ProtocolRegistry, is_dotted_lite_username,
		namehash_under,
	},
	platform::is_dns_label,
	runtime::{DotnsLookup, DotnsLookupKind, services::RuntimeServices},
};

/// Text record a username publishes its profile at.
const PROFILE_RECORD_KEY: &str = "profile";
/// How long a resolved record is served before the chain is read again.
#[cfg(not(target_arch = "wasm32"))]
const RECORD_TTL_SECS: u64 = 60 * 60;

/// Controls profile metadata freshness and the initial stream value.
///
/// Options affect metadata resolution; image bytes remain cached separately by CID.
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PublicProfileResolutionOptions {
	/// Reads dotNS even when stored metadata is still fresh.
	pub force_fresh: bool,
	/// Suppresses a stale stored profile before resolving through dotNS.
	pub skip_stale: bool,
}

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
#[derive(Debug, PartialEq)]
struct ProfileRecordLocation {
	/// Username namehash under the deployment's TLD.
	node: [u8; 32],
	/// Forward registry that determines the node's owner and delegates.
	registry: [u8; 20],
	/// Content resolver registered by the deployment.
	resolver: [u8; 20],
}

impl ProfileRecordLocation {
	/// Finds the profile's content resolver without changing the node's resolver.
	async fn resolve<T: DotnsTransport + ?Sized>(
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
	async fn read<T: DotnsTransport + ?Sized>(
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

	/// Resolves public profiles with optional cached-first delivery.
	///
	/// Options can force a lookup and suppress stale stored metadata. A resolved value
	/// is emitted when no stored value was emitted or when the profile differs.
	/// Dropping the stream cancels resolution. At most two profiles are emitted.
	#[cfg(not(target_arch = "wasm32"))]
	pub fn observe(
		self,
		username: String,
		options: PublicProfileResolutionOptions,
	) -> Subscription<PublicProfile, PublicProfileError> {
		if let Err(error) = validate_username(&username) {
			return Subscription::interrupted(error);
		}
		let PublicProfileResolutionOptions { force_fresh, skip_stale } = options;
		enum Resolution {
			Stored,
			Refresh(ProfileRecord),
		}

		Subscription::new(stream::try_unfold(
			Some((self, username, Resolution::Stored)),
			move |state| async move {
				let Some((profiles, username, resolution)) = state else { return Ok(None) };
				match resolution {
					Resolution::Stored => {
						if let Some(cached) = profiles.stored_record(&username).await? {
							let fresh = cached.is_fresh(current_unix_secs());
							if fresh || !skip_stale {
								let profile = PublicProfile { avatar: cached.record.avatar() };
								let next = (force_fresh || !fresh).then_some((
									profiles,
									username,
									Resolution::Refresh(cached.record),
								));
								return Ok(Some((profile, next)));
							}
						}
						let profile = profiles.refresh_profile(&username).await?;
						Ok(Some((profile, None)))
					},
					Resolution::Refresh(previous) => {
						let profile = profiles.refresh_profile(&username).await?;
						let previous = PublicProfile { avatar: previous.avatar() };
						if profile == previous { Ok(None) } else { Ok(Some((profile, None))) }
					},
				}
			},
		))
	}

	/// Resolves one profile without native storage.
	///
	/// Browsers have no profile cache, so the options have no effect. Resolution
	/// failures terminate the stream; dropping the stream cancels the lookup.
	#[cfg(target_arch = "wasm32")]
	pub fn observe(
		self,
		username: String,
		_options: PublicProfileResolutionOptions,
	) -> Subscription<PublicProfile, PublicProfileError> {
		Subscription::new(stream::once(async move {
			validate_username(&username)?;
			let record = self.read_record(&username).await?;
			Ok(PublicProfile { avatar: record.avatar() })
		}))
	}

	#[cfg(not(target_arch = "wasm32"))]
	async fn stored_record(
		&self,
		username: &str,
	) -> Result<Option<dao::CachedRecord>, PublicProfileError> {
		let db = self.services.core_db()?;
		let username = username.to_owned();
		Ok(db.write(move |tx| dao::stored_record(tx, &username)).await?)
	}

	#[cfg(not(target_arch = "wasm32"))]
	async fn refresh_profile(&self, username: &str) -> Result<PublicProfile, PublicProfileError> {
		let db = self.services.core_db()?;
		let record = self.read_record(username).await?;
		let profile = PublicProfile { avatar: record.avatar() };
		let username = username.to_owned();
		let expires_at = current_unix_secs() + RECORD_TTL_SECS;
		db.write(move |tx| dao::store_record(tx, &username, &record, expires_at))
			.await?;
		Ok(profile)
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
