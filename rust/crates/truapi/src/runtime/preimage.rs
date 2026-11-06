//! Bulletin submission shared by product calls and the host's identity profile.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use subxt::tx::Signer;
use tracing::{info, warn};
use truapi::CallContext;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use super::{
	AuthoritySession, BulletinAllowanceKey, PREIMAGE_BULLETIN_AUTHORIZATION_WAIT,
	PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT, PREIMAGE_SUBMIT_TIMEOUT, ProductAccounts,
	RuntimeServices, bulletin_allowance_error_reason,
	bulletin_rpc::BulletinSubmitError,
	remote_authority_call, remote_authority_context_until,
	statement_allowance::{fetch_bulletin_allowance, rpc::RpcClient, wait_bulletin_authorization},
};
use crate::host_internal::bulletin::{allowance_signer, preimage_key};

/// A submission charged to the accounts of the authenticated caller.
///
/// Uses the caller's active session to acquire and renew Bulletin allowance.
pub struct PreimageSubmission<'a> {
	services: &'a RuntimeServices,
	accounts: &'a ProductAccounts,
	session: &'a AuthoritySession,
}

impl<'a> PreimageSubmission<'a> {
	/// Bind the submission to the session whose allowance may be spent.
	pub fn new(
		services: &'a RuntimeServices,
		accounts: &'a ProductAccounts,
		session: &'a AuthoritySession,
	) -> Self {
		Self { services, accounts, session }
	}

	/// Returns the content key after successful storage, priming local lookups.
	///
	/// Acquires sufficient allowance and retries once if Bulletin rejects it. Authorization,
	/// timeout and submission failures return their reason without caching the bytes.
	pub async fn submit(&self, cx: &CallContext, value: Vec<u8>) -> Result<[u8; 32], String> {
		let deadline = Instant::now() + PREIMAGE_SUBMIT_TIMEOUT;
		let authority_cx = remote_authority_context_until(
			cx,
			PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
			deadline,
		);
		let allowance = remote_authority_call(
			&authority_cx,
			self.accounts.bulletin_allowance_key(&authority_cx, self.session),
		)
		.await
		.map_err(bulletin_allowance_error_reason)?;

		#[cfg(feature = "test-host")]
		if self.accounts.submits_preimages_locally() {
			let key = preimage_key(&value);
			self.services.keep_local_preimage(key, value);
			return Ok(key);
		}

		let size = u64::try_from(value.len()).map_err(|_| "preimage length exceeds u64")?;
		let allowance = self.allowance_for(cx, deadline, allowance, size).await?;
		let key =
			match self.services.bulletin.submit_preimage(cx, deadline, &allowance, &value).await {
				Ok(key) => key,
				Err(BulletinSubmitError::AllowanceRejected { phase }) => {
					warn!(%phase, "Bulletin rejected the preimage allowance; extending it and resubmitting once");
					let allowance = self.refresh_allowance(cx, deadline).await?;
					self.services
						.bulletin
						.submit_preimage(cx, deadline, &allowance, &value)
						.await
						.map_err(|error| error.to_string())?
				},
				Err(error) => return Err(error.to_string()),
			};
		let key: [u8; 32] =
			key.try_into().map_err(|_| "Bulletin returned an invalid preimage key")?;
		debug_assert_eq!(key, preimage_key(&value));
		self.services.cache_preimage(key, value);
		Ok(key)
	}

	async fn refresh_allowance(
		&self,
		cx: &CallContext,
		deadline: Instant,
	) -> Result<BulletinAllowanceKey, String> {
		let authority_cx = remote_authority_context_until(
			cx,
			PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
			deadline,
		);
		remote_authority_call(
			&authority_cx,
			self.accounts.refresh_bulletin_allowance_key(&authority_cx, self.session),
		)
		.await
		.map_err(bulletin_allowance_error_reason)
	}

	async fn allowance_for(
		&self,
		cx: &CallContext,
		deadline: Instant,
		allowance: BulletinAllowanceKey,
		size: u64,
	) -> Result<BulletinAllowanceKey, String> {
		let rpc = RpcClient::new(
			self.services
				.bulletin
				.client("Bulletin authorization")
				.await
				.map_err(|error| error.to_string())?,
		);
		let target = allowance_signer(&allowance)?.account_id().0;
		let current = fetch_bulletin_allowance(&rpc, &target)
			.await
			.map_err(|error| error.to_string())?;
		if current.is_some_and(|info| info.can_store(size)) {
			info!(size, "Bulletin authorization sufficient for the preimage");
			return Ok(allowance);
		}
		info!(size, ?current, "extending Bulletin authorization before submitting the preimage");
		let allowance = self.refresh_allowance(cx, deadline).await?;
		let replacement = allowance_signer(&allowance)?.account_id().0;
		let baseline = (replacement == target).then_some(current).flatten();
		let timeout = PREIMAGE_BULLETIN_AUTHORIZATION_WAIT
			.min(deadline.saturating_duration_since(Instant::now()));
		wait_bulletin_authorization(&rpc, &replacement, baseline, size, timeout)
			.await
			.map_err(|error| error.to_string())?;
		Ok(allowance)
	}
}
