//! Product-facing preimage capability adapters.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use futures::StreamExt;
use tracing::{instrument, warn};
use truapi::{
	CallContext, CallError, Subscription,
	api::Preimage,
	latest,
	versioned::preimage::{
		RemotePreimageLookupSubscribeError, RemotePreimageLookupSubscribeItem,
		RemotePreimageLookupSubscribeRequest, RemotePreimageSubmitError,
		RemotePreimageSubmitRequest, RemotePreimageSubmitResponse,
	},
};

use crate::{
	host_internal::bulletin::preimage_key,
	runtime::{
		PERMISSION_DENIED_REASON, PREIMAGE_SUBMIT_TIMEOUT, ProductPreimageError,
		ProductRuntimeHost, preimage_submit_error, versioning::version,
	},
};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

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
		version(request)
			.exec(|latest::RemotePreimageLookupSubscribeRequest { key }| async move {
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
				let stream =
					self.connection.platform.lookup_preimage(key.clone()).map(move |item| {
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
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "preimage.submit"))]
	async fn submit(
		&self,
		cx: &CallContext,
		request: RemotePreimageSubmitRequest,
	) -> Result<RemotePreimageSubmitResponse, CallError<RemotePreimageSubmitError>> {
		version(request)
			.exec(|value| async move {
				let Some(session) = self.accounts.current_session() else {
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
				self.store_product_preimage(cx, &session, value, submission_deadline)
					.await
					.map_err(|error| match error {
						ProductPreimageError::Allowance(reason) |
						ProductPreimageError::Submit(reason) => preimage_submit_error(reason),
					})
			})
			.await
	}
}
