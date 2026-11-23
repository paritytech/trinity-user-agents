//! Consent-free resolution of Media's immutable authorization scope.

use core::time::Duration;

use futures::{FutureExt, pin_mut};
use truapi::{CallContext, CancellationReason};
use crate::platform::{ProductContext, normalize_product_identifier};

use crate::host_logic::media_protocol::{MAX_PRODUCT_ID_BYTES, MediaIdentity};
use crate::host_logic::product_account::{derive_product_public_key, index_bytes};

use super::{
    AuthorityError, AuthoritySession, ProductAuthority, RuntimeServices,
    authority_cancellation_error,
};

const IDENTITY_TIMEOUT: Duration = Duration::from_secs(5);

/// Resolve only the configured network and the active authority's canonical
/// product Index(0). This neither opens the backend nor advertises an endpoint.
pub(super) async fn resolve_identity(
    services: &RuntimeServices,
    authority: &dyn ProductAuthority,
    product: &ProductContext,
    cx: &CallContext,
) -> Result<(AuthoritySession, MediaIdentity), AuthorityError> {
    if let Some(reason) = cx.cancel().reason() {
        return Err(authority_cancellation_error(cx, reason));
    }
    let session = authority.current_session().ok_or(AuthorityError::Disconnected)?;
    let product_id = product.product_id.as_str();
    if product_id.len() > MAX_PRODUCT_ID_BYTES
        || normalize_product_identifier(product_id).map_err(|_| AuthorityError::Rejected)?
            != product_id
    {
        return Err(AuthorityError::Rejected);
    }
    let network = services.statement_store.genesis_hash();
    if network == [0; 32] {
        return Err(AuthorityError::Unavailable {
            reason: "Media network is not configured".into(),
        });
    }

    let timeout = cx.timeout().unwrap_or(IDENTITY_TIMEOUT).min(IDENTITY_TIMEOUT);
    let mut bounded = cx.clone();
    bounded.set_timeout(timeout);
    let subtree = {
        let subtree = authority
            .product_subtree_public_key(&bounded, &session, product_id.to_owned())
            .fuse();
        let cancelled = bounded.cancel().cancelled().fuse();
        let deadline = futures_timer::Delay::new(timeout).fuse();
        pin_mut!(subtree, cancelled, deadline);
        // Unlike remote_authority_call, there is no additional unwind grace: the
        // identity path has a hard five-second budget, even with a larger caller one.
        futures::select_biased! {
            reason = cancelled => return Err(authority_cancellation_error(cx, reason)),
            () = deadline => {
                let reason = CancellationReason::TimedOut { timeout };
                bounded.cancel().cancel_with_reason(reason.clone());
                return Err(authority_cancellation_error(cx, reason));
            },
            result = subtree => result?,
        }
    };
    if authority.current_session().as_ref() != Some(&session) {
        return Err(AuthorityError::Disconnected);
    }
    if let Some(reason) = cx.cancel().reason() {
        return Err(authority_cancellation_error(cx, reason));
    }
    let account = derive_product_public_key(subtree, index_bytes(0))
        .map_err(|_| AuthorityError::Rejected)?;
    Ok((session, MediaIdentity { network, product_id: product_id.to_owned(), account }))
}
