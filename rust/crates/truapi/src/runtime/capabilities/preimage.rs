//! Product-facing preimage capability adapters.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use crate::platform::{PreimageSubmitReview, UserConfirmationReview};
use futures::StreamExt;
use tracing::{instrument, warn};
use truapi::api::Preimage;
use truapi::versioned::preimage::{
    RemotePreimageLookupSubscribeError, RemotePreimageLookupSubscribeItem,
    RemotePreimageLookupSubscribeRequest, RemotePreimageSubmitError, RemotePreimageSubmitRequest,
    RemotePreimageSubmitResponse,
};
use truapi::{CallContext, CallError, Subscription, v01};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::host_internal::bulletin::preimage_key;
use crate::runtime::bulletin_rpc::BulletinSubmitError;
use crate::runtime::{
    PERMISSION_DENIED_REASON, PREIMAGE_REMOTE_AUTHORITY_RESPONSE_TIMEOUT, PREIMAGE_SUBMIT_TIMEOUT,
    ProductRuntimeHost, bulletin_allowance_error_reason, preimage_submit_error,
    remote_authority_call, remote_authority_context_until, until_cancelled,
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
        if let Ok(key_bytes) = <[u8; 32]>::try_from(key.as_slice())
            && let Some(value) = self.services.cached_preimage(&key_bytes)
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
                    return Err(CallError::HostFailure {
                        reason: error.reason,
                    });
                }
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
            Ok(RemotePreimageLookupSubscribeItem::V1(
                v01::RemotePreimageLookupSubscribeItem { value },
            ))
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
        let confirmed = until_cancelled(
            cx,
            self.confirm_product_action(UserConfirmationReview::PreimageSubmit(
                PreimageSubmitReview {
                    size: value.len() as u64,
                },
            )),
        )
        .await
        .map_err(|err| preimage_submit_error(err.to_string()))?
        .map_err(|err| preimage_submit_error(err.reason))?;
        if !confirmed {
            return Err(preimage_submit_error(
                "User rejected preimage submission".to_string(),
            ));
        }
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

        // A test host's allowance was never authorized on chain, so a Bulletin
        // `store` signed with it would always be refused. The value is kept in
        // the core under its content key, and the product reads it back through
        // the same lookup a landed submission answers.
        #[cfg(feature = "test-host")]
        if self.authority.submits_preimages_locally() {
            let key = preimage_key(&value);
            self.services.keep_local_preimage(key, value);
            return Ok(RemotePreimageSubmitResponse::V1(key.to_vec()));
        }

        let key = match bulletin
            .submit_preimage(cx, submission_deadline, &allowance, &value)
            .await
        {
            Ok(key) => key,
            // A rejected allowance is the one case a refresh-and-retry can fix:
            // evict the exhausted key, allocate a fresh (increased) allowance,
            // and try exactly once more.
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
            }
            Err(err) => return Err(preimage_submit_error(err.to_string())),
        };
        // Move the owned body into the lookup cache (no extra copy) so an
        // immediate product lookup hits before the content backend has it.
        self.prime_preimage_cache(&key, value);
        Ok(RemotePreimageSubmitResponse::V1(key))
    }
}
