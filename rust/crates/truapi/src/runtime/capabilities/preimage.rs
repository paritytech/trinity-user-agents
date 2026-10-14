//! Product-facing preimage capability adapters.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use futures::StreamExt;
use tracing::{instrument, warn};
use truapi::{
	CallContext, CallError, Subscription,
	api::Preimage,
	v01,
	versioned::preimage::{
		RemotePreimageCacheAccountRequest, RemotePreimageCacheAccountResponse,
		RemotePreimageLookupSubscribeError, RemotePreimageLookupSubscribeItem,
		RemotePreimageLookupSubscribeRequest, RemotePreimageReadError, RemotePreimageReadRequest,
		RemotePreimageReadResponse, RemotePreimageReadTorrentRequest,
		RemotePreimageReadTorrentResponse, RemotePreimageSubmitError, RemotePreimageSubmitRequest,
		RemotePreimageSubmitResponse,
	},
};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::{
	host_internal::bulletin::preimage_key,
	runtime::{
		PERMISSION_DENIED_REASON, PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
		PREIMAGE_SUBMIT_TIMEOUT, ProductRuntimeHost, bulletin_allowance_error_reason,
		bulletin_rpc::BulletinSubmitError, preimage_submit_error, remote_authority_call,
		remote_authority_context_until,
	},
};

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
		let RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
			key,
		}) = request;

		// A cache hit is final: preimages are content-addressed and immutable.
		// Emit the value once, then keep the subscription open until the
		// caller unsubscribes, since there is nothing left to report.
		if let Ok(key_bytes) = <[u8; 32]>::try_from(key.as_slice()) &&
			let Some(value) = self.services.cached_preimage(&key_bytes)
		{
			let item =
				RemotePreimageLookupSubscribeItem::V1(v01::RemotePreimageLookupSubscribeItem {
					value: Some(value),
				});
			let stream =
				futures::stream::once(async move { Ok(item) }).chain(futures::stream::pending());
			return Subscription::new(stream);
		}

		// Otherwise delegate to the host content backend, verifying that any
		// returned value hashes to the requested key so a compromised backend
		// cannot feed products forged content. A mismatch is reported as a
		// miss, so the product still gets its initial current-value/miss
		// emission.
		let stream = self.platform.lookup_preimage(key.clone()).map(move |item| {
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
			Ok(RemotePreimageLookupSubscribeItem::V1(v01::RemotePreimageLookupSubscribeItem {
				value,
			}))
		});
		Subscription::new(stream)
	}

	#[instrument(skip_all, fields(runtime.method = "preimage.submit"))]
	async fn submit(
		&self,
		cx: &CallContext,
		request: RemotePreimageSubmitRequest,
	) -> Result<RemotePreimageSubmitResponse, CallError<RemotePreimageSubmitError>> {
		let RemotePreimageSubmitRequest::V1(value) = request;
		let Some(session) = self.authority.current_session() else {
			return Err(preimage_submit_error("No active session".to_string()));
		};
		let bulletin = &self.services.bulletin;
		self.require_remote_permission(
			v01::RemotePermission::PreimageSubmit,
			RemotePreimageSubmitError::V1(v01::PreimageSubmitError::Unknown {
				reason: PERMISSION_DENIED_REASON.to_string(),
			}),
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
			self.authority
				.bulletin_allowance_key(&authority_cx, &session, self.product_id()),
		)
		.await
		.map_err(|err| preimage_submit_error(bulletin_allowance_error_reason(err)))?;

		#[cfg(feature = "test-host")]
		if self.authority.submits_preimages_locally() {
			let key = preimage_key(&value);
			self.services.keep_local_preimage(key, value);
			return Ok(RemotePreimageSubmitResponse::V1(key.to_vec()));
		}

		let key = match bulletin.submit_preimage(cx, submission_deadline, &allowance, &value).await
		{
			Ok(key) => key,
			Err(BulletinSubmitError::AllowanceRejected { .. }) => {
				let authority_cx = remote_authority_context_until(
					cx,
					PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT,
					submission_deadline,
				);
				let allowance = remote_authority_call(
					&authority_cx,
					self.authority.refresh_bulletin_allowance_key(
						&authority_cx,
						&session,
						self.product_id(),
					),
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
		Ok(RemotePreimageSubmitResponse::V1(key))
	}

	#[instrument(skip_all, fields(runtime.method = "preimage.read"))]
	async fn read(
		&self,
		_cx: &CallContext,
		request: RemotePreimageReadRequest,
	) -> Result<RemotePreimageReadResponse, CallError<RemotePreimageReadError>> {
		let RemotePreimageReadRequest::V1(v01::RemotePreimageReadRequest {
			key,
			route,
			skip_host_caches,
		}) = request;
		let started = Instant::now();

		// The read-after-write cache answers first, as for a lookup, unless the
		// product asks to skip host caches.
		if !skip_host_caches &&
			let Ok(key_bytes) = <[u8; 32]>::try_from(key.as_slice()) &&
			let Some(value) = self.services.cached_preimage(&key_bytes)
		{
			let ms = elapsed_ms(started);
			return Ok(RemotePreimageReadResponse::V1(v01::RemotePreimageReadResponse {
				value: Some(value),
				report: v01::PreimageReadReport {
					served_by: Some(v01::PreimageReadSource::HostCache),
					attempts: vec![v01::PreimageReadAttempt {
						source: v01::PreimageReadAttemptSource::HostCache,
						outcome: v01::PreimageReadOutcome::Served,
						ms,
					}],
					host_ms: ms,
				},
			}));
		}

		let Some(host) = self.services.preimage_read_host() else {
			return Err(CallError::Unsupported);
		};
		let mut response = host
			.read_preimage(key.clone(), route, skip_host_caches)
			.await
			.map_err(|error| CallError::Domain(RemotePreimageReadError::V1(error)))?;
		// The host checks the bytes, and the core checks them again, so a
		// compromised backend cannot feed products forged content.
		if response.value.as_ref().is_some_and(|value| preimage_key(value)[..] != key[..]) {
			warn!("preimage read returned a value whose hash does not match the requested key");
			mark_bad_bytes(&mut response.report);
			response.value = None;
		}
		response.report.host_ms = elapsed_ms(started);
		Ok(RemotePreimageReadResponse::V1(response))
	}

	#[instrument(skip_all, fields(runtime.method = "preimage.read_torrent"))]
	async fn read_torrent(
		&self,
		_cx: &CallContext,
		request: RemotePreimageReadTorrentRequest,
	) -> Result<RemotePreimageReadTorrentResponse, CallError<RemotePreimageReadError>> {
		let RemotePreimageReadTorrentRequest::V1(request) = request;
		if request.route == v01::PreimageReadRoute::Bulletin {
			return Err(CallError::Domain(RemotePreimageReadError::V1(
                v01::PreimageReadError::Unknown {
                    reason: "a torrent is not on Bulletin: use the route Auto, Cache or a cache provider"
                        .to_string(),
                },
            )));
		}
		let started = Instant::now();
		let whole = request.range.is_none();
		let Some(host) = self.services.preimage_read_host() else {
			return Err(CallError::Unsupported);
		};
		let mut response = host
			.read_torrent(request)
			.await
			.map_err(|error| CallError::Domain(RemotePreimageReadError::V1(error)))?;
		// The host checked the file against the piece hashes of the torrent. For a
		// whole value, the core derives the preimage key from the bytes itself, and
		// keeps the file in its read-after-write cache, as after a submit. A part is
		// not the whole file, so the key of the host stays.
		if whole && let Some(value) = &response.value {
			let key = preimage_key(value);
			response.key = Some(key);
			response.size = Some(value.len() as u64);
			self.prime_preimage_cache(&key, value.clone());
		}
		response.report.host_ms = elapsed_ms(started);
		Ok(RemotePreimageReadTorrentResponse::V1(response))
	}

	#[instrument(skip_all, fields(runtime.method = "preimage.cache_account"))]
	async fn cache_account(
		&self,
		_cx: &CallContext,
		request: RemotePreimageCacheAccountRequest,
	) -> Result<RemotePreimageCacheAccountResponse, CallError<RemotePreimageReadError>> {
		let RemotePreimageCacheAccountRequest::V1(request) = request;
		let Some(host) = self.services.preimage_read_host() else {
			return Err(CallError::Unsupported);
		};
		host.cache_account(request)
			.await
			.map(RemotePreimageCacheAccountResponse::V1)
			.map_err(|error| CallError::Domain(RemotePreimageReadError::V1(error)))
	}
}

/// Milliseconds since `started`, at most `u32::MAX`.
fn elapsed_ms(started: Instant) -> u32 {
	u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

/// Record that the source that served a read sent bytes that do not match the
/// key. Its attempt becomes `BadBytes`, and the report names no source that
/// served.
fn mark_bad_bytes(report: &mut v01::PreimageReadReport) {
	report.served_by = None;
	if let Some(attempt) = report
		.attempts
		.iter_mut()
		.rev()
		.find(|attempt| attempt.outcome == v01::PreimageReadOutcome::Served)
	{
		attempt.outcome = v01::PreimageReadOutcome::BadBytes;
	}
}
