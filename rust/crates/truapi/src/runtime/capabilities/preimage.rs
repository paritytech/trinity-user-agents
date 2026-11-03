//! Product-facing preimage capability adapters.

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
		PERMISSION_DENIED_REASON, ProductRuntimeHost, preimage::PreimageSubmission,
		preimage_submit_error, versioning::version,
	},
};

#[async_trait::async_trait]
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
				// Emit an answer that cannot change, then keep the subscription
				// open until the caller unsubscribes, since there is nothing left
				// to report.
				let final_answer = |value| {
					let item = latest::RemotePreimageLookupSubscribeItem { value };
					let stream = futures::stream::once(async move { Ok(item) })
						.chain(futures::stream::pending());
					Subscription::new(stream)
				};

				// A preimage key is a blake2b-256 digest, so no value can answer a
				// key of any other length.
				let Ok(key) = <[u8; 32]>::try_from(key.as_slice()) else {
					return final_answer(None);
				};

				// A cache hit is final: preimages are content-addressed and immutable.
				if let Some(value) = self.connection.services.cached_preimage(&key) {
					return final_answer(Some(value));
				}

				// Otherwise delegate to the host content backend, verifying that any
				// returned value hashes to the requested key so a compromised backend
				// cannot feed products forged content. A mismatch is reported as a
				// miss, so the product still gets its initial current-value/miss
				// emission.
				let stream = self.connection.platform.lookup_preimage(key).map(move |item| {
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
						let matches = preimage_key(value) == key;
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

	#[instrument(skip_all, fields(runtime.method = "preimage.submit", product = %self.connection.product_id()))]
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
				PreimageSubmission::new(&self.connection.services, &self.accounts, &session)
					.submit(cx, value)
					.await
					.map(|key| key.to_vec())
					.map_err(preimage_submit_error)
			})
			.await
	}
}
