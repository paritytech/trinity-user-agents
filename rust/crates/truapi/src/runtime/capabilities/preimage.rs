//! Product-facing preimage capability adapters.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use futures::StreamExt;
use tracing::{instrument, warn};
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
	host_internal::bulletin::preimage_key,
	runtime::{
		PERMISSION_DENIED_REASON, PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
		PREIMAGE_SUBMIT_TIMEOUT, ProductRuntimeHost, preimage_submit_error, remote_authority_call,
		remote_authority_context_until,
	},
};

#[truapi::async_trait]
impl<H: crate::runtime::AccountHolder> Preimage for ProductRuntimeHost<H> {
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
			let Some(authority_session) = self.accounts.current_session() else {
				return Err(preimage_submit_error("No active session".to_string()));
			};
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
			let key = remote_authority_call(
				&authority_cx,
				self.accounts.submit_preimage(
					&authority_cx,
					submission_deadline,
					&authority_session,
					self.connection.product_id(),
					&value,
				),
			)
			.await
			.map_err(|err| preimage_submit_error(err.to_string()))?;
			#[cfg(feature = "test-host")]
			if self.accounts.submits_preimages_locally() {
				self.connection.services.keep_local_preimage(preimage_key(&value), value);
				return Ok(key);
			}

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
