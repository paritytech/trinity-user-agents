//! Product-facing preimage capability adapters.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use futures::StreamExt;
use tracing::{info, instrument, warn};
use truapi::{
	CallContext, CallError, Subscription,
	api::Preimage,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		preimage::{
			RemotePreimageLookupSubscribeError, RemotePreimageLookupSubscribeItem,
			RemotePreimageLookupSubscribeRequest, RemotePreimageSubmitError,
			RemotePreimageSubmitRequest, RemotePreimageSubmitResponse,
		},
	},
};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use super::from_latest_error;
use crate::{
	host_internal::bulletin::{allowance_signer, preimage_key},
	runtime::{
		AuthoritySession, BulletinAllowanceKey, PERMISSION_DENIED_REASON,
		PREIMAGE_BULLETIN_AUTHORIZATION_WAIT, PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
		PREIMAGE_SUBMIT_TIMEOUT, ProductRuntimeHost, bulletin_allowance_error_reason,
		bulletin_rpc::BulletinSubmitError,
		preimage_submit_error, remote_authority_call, remote_authority_context_until,
		statement_allowance::{
			fetch_bulletin_allowance, rpc::RpcClient, wait_bulletin_authorization,
		},
	},
};
use subxt::tx::Signer;

impl ProductRuntimeHost {
	/// Extend the product's Bulletin authorization before signing a payload it
	/// cannot hold, so the chain never rejects a submission the host could see coming.
	async fn bulletin_allowance_for(
		&self,
		cx: &CallContext,
		deadline: Instant,
		session: &AuthoritySession,
		allowance: BulletinAllowanceKey,
		size: u64,
	) -> Result<BulletinAllowanceKey, String> {
		let rpc = RpcClient::new(
			self.connection
				.services
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
			info!(
				product = %self.connection.product_id(),
				size,
				"Bulletin authorization sufficient for the preimage"
			);
			return Ok(allowance);
		}
		info!(
			product = %self.connection.product_id(),
			size,
			?current,
			"extending Bulletin authorization before submitting the preimage"
		);
		let authority_cx = remote_authority_context_until(
			cx,
			PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
			deadline,
		);
		let allowance = remote_authority_call(
			&authority_cx,
			self.accounts.refresh_bulletin_allowance_key(&authority_cx, session),
		)
		.await
		.map_err(bulletin_allowance_error_reason)?;
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

#[truapi::async_trait]
impl Preimage for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "preimage.lookup_subscribe"))]
	async fn lookup_subscribe(
		&self,
		_cx: &CallContext,
		request: RemotePreimageLookupSubscribeRequest,
	) -> Subscription<
		RemotePreimageLookupSubscribeItem,
		CallError<RemotePreimageLookupSubscribeError>,
	> {
		let version = request.version();
		let request = request.into_latest();
		let subscription: Subscription<
			latest::RemotePreimageLookupSubscribeItem,
			CallError<latest::GenericError>,
		> = async {
			let latest::RemotePreimageLookupSubscribeRequest { key } = request;

			// A cache hit is final: preimages are content-addressed and immutable.
			// Emit the value once, then keep the subscription open until the
			// caller unsubscribes, since there is nothing left to report.
			if let Ok(key_bytes) = <[u8; 32]>::try_from(key.as_slice()) &&
				let Some(value) = self.connection.services.cached_preimage(&key_bytes)
			{
				let item = latest::RemotePreimageLookupSubscribeItem { value: Some(value) };
				let stream = futures::stream::once(async move { Ok(item) })
					.chain(futures::stream::pending());
				return Subscription::new(stream);
			}

			// Otherwise delegate to the host content backend, verifying that any
			// returned value hashes to the requested key so a compromised backend
			// cannot feed products forged content. A mismatch is reported as a
			// miss, so the product still gets its initial current-value/miss
			// emission.
			let stream = self.connection.platform.lookup_preimage(key.clone()).map(move |item| {
				let value = match item {
					Ok(value) => value,
					Err(error) => {
						warn!(
							reason = %error.reason,
							"preimage lookup platform stream failed"
						);
						return Err(CallError::HostFailure { reason: error.reason });
					},
				};
				let value = value.filter(|value| {
					let matches = preimage_key(value)[..] == key[..];
					if !matches {
						warn!(
							"preimage lookup returned a value whose hash does not match the \
	                         requested key; downgrading to a miss"
						);
					}
					matches
				});
				Ok(latest::RemotePreimageLookupSubscribeItem { value })
			});
			Subscription::new(stream)
		}
		.await;
		Subscription::new(subscription.map(move |result| {
			result
				.map(|item| RemotePreimageLookupSubscribeItem::from_latest(item, version))
				.map_err(|error| from_latest_error(error, version))
		}))
	}

	#[instrument(skip_all, fields(runtime.method = "preimage.submit"))]
	async fn submit(
		&self,
		cx: &CallContext,
		request: RemotePreimageSubmitRequest,
	) -> Result<RemotePreimageSubmitResponse, CallError<RemotePreimageSubmitError>> {
		let version = request.version();
		let value = request.into_latest();
		let result = async {
			let Some(session) = self.accounts.current_session() else {
				return Err(preimage_submit_error("No active session".to_string()));
			};
			let bulletin = &self.connection.services.bulletin;
			self.connection
				.require_remote_permission(
					latest::RemotePermission::PreimageSubmit,
					latest::PreimageSubmitError::Unknown {
						reason: PERMISSION_DENIED_REASON.to_string(),
					},
				)
				.await?;
			let submission_deadline = Instant::now() + PREIMAGE_SUBMIT_TIMEOUT;
			let authority_cx = remote_authority_context_until(
				cx,
				PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
				submission_deadline,
			);
			let allowance = remote_authority_call(
				&authority_cx,
				self.accounts.bulletin_allowance_key(&authority_cx, &session),
			)
			.await
			.map_err(|err| preimage_submit_error(bulletin_allowance_error_reason(err)))?;

			#[cfg(feature = "test-host")]
			if self.accounts.submits_preimages_locally() {
				let key = preimage_key(&value);
				self.connection.services.keep_local_preimage(key, value);
				return Ok(key.to_vec());
			}

			let allowance = self
				.bulletin_allowance_for(
					cx,
					submission_deadline,
					&session,
					allowance,
					value.len() as u64,
				)
				.await
				.map_err(preimage_submit_error)?;
			let key = match bulletin
				.submit_preimage(cx, submission_deadline, &allowance, &value)
				.await
			{
				Ok(key) => key,
				Err(BulletinSubmitError::AllowanceRejected { phase }) => {
					warn!(
						product = %self.connection.product_id(),
						%phase,
						"Bulletin rejected the preimage allowance; extending it and resubmitting once"
					);
					let authority_cx = remote_authority_context_until(
						cx,
						PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
						submission_deadline,
					);
					let allowance = remote_authority_call(
						&authority_cx,
						self.accounts.refresh_bulletin_allowance_key(&authority_cx, &session),
					)
					.await
					.map_err(|err| preimage_submit_error(bulletin_allowance_error_reason(err)))?;
					bulletin
						.submit_preimage(cx, submission_deadline, &allowance, &value)
						.await
						.map_err(|err| preimage_submit_error(err.to_string()))?
				},
				Err(err) => return Err(preimage_submit_error(err.to_string())),
			};
			// Move the owned body into the lookup cache (no extra copy) so an
			// immediate product lookup hits before the content backend has it.
			self.prime_preimage_cache(&key, value);
			Ok(key)
		}
		.await;
		result
			.map(|response| RemotePreimageSubmitResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}
