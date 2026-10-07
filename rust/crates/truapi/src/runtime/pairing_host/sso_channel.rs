//! SSO statement-store channel to the paired remote signing host.

use super::super::authority::{
    AccountCaller, AccountHolder, AccountInvocation, AuthorityError, BulletinAllowanceKey,
    StatementStoreAllowanceKey, authority_session,
};
use super::super::sso_remote::{SsoSessionKey, sso_message_id};
use super::PairingHost;
use crate::host_internal::sso_messages::{
    OnExistingAllowancePolicy, ProductRequest, SsoAllocatedResource, SsoAllocationOutcome,
};
use crate::host_logic::session::SessionInfo;
use tracing::warn;
use truapi::{CallContext, latest};

impl PairingHost {
    /// Allocate exactly one allowance for the product and return its material.
    async fn remote_allowance_slot(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        product_id: &str,
        resource: latest::AllocatableResource,
        on_existing: OnExistingAllowancePolicy,
    ) -> Result<SsoAllocatedResource, AuthorityError> {
        let name = allowance_name(&resource);
        let outcomes = self
            .holder
            .allocate_resources(
                cx,
                &authority_session(session),
                product_id.to_string(),
                latest::HostRequestResourceAllocationRequest {
                    resources: vec![resource],
                },
                on_existing,
            )
            .await?;
        match outcomes.into_iter().next() {
            Some(SsoAllocationOutcome::Allocated(resource)) => Ok(resource),
            Some(SsoAllocationOutcome::Rejected) => Err(AuthorityError::Rejected),
            Some(SsoAllocationOutcome::NotAvailable) => Err(AuthorityError::Unavailable {
                reason: format!("{name} is not available"),
            }),
            None => Err(AuthorityError::Unknown {
                reason: format!("Empty {name} response"),
            }),
        }
    }

    async fn allocate_bulletin_allowance_key(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        lifecycle_epoch: u64,
        product_id: String,
        on_existing: OnExistingAllowancePolicy,
    ) -> Result<BulletinAllowanceKey, AuthorityError> {
        match self
            .remote_allowance_slot(
                cx,
                session,
                &product_id,
                latest::AllocatableResource::BulletinAllowance,
                on_existing,
            )
            .await?
        {
            SsoAllocatedResource::BulletinAllowance { slot_account_key } => {
                self.grants
                    .cache_bulletin_allowance_key(
                        &self.sso.session_state(),
                        session,
                        lifecycle_epoch,
                        &product_id,
                        slot_account_key,
                    )
                    .await
            }
            other => Err(unexpected_resource("bulletin allowance", &other)),
        }
    }
}

/// Mirror a local registration to the paired account holder.
pub fn mirror_ring_vrf_registration(
    host: &PairingHost,
    session: SessionInfo,
    request: ProductRequest<latest::HostAccountRegisterRingVrfKeyRequest>,
) {
    let weak_self = std::sync::Arc::downgrade(&host.holder);
    (host.services.spawner)(Box::pin(async move {
        let Some(host) = weak_self.upgrade() else {
            return;
        };
        let cx = CallContext::with_request_id(format!(
            "ring-vrf-registration-mirror:{}",
            sso_message_id()
        ));
        if let Err(error) = host
            .register_ring_vrf_key(
                AccountInvocation {
                    call: &cx,
                    session: &authority_session(&session),
                    caller: AccountCaller::Remote {
                        product_id: Some(&request.calling_product_id),
                    },
                },
                request.payload,
            )
            .await
        {
            warn!(?error, "ring-VRF registration mirror failed");
        }
    }));
}

/// Resolve a product's hard-subtree public key, asking the Account Holder
/// only when neither the memory cache nor storage already holds it.
pub async fn remote_product_subtree_public_key(
    host: &PairingHost,
    cx: &CallContext,
    session: &SessionInfo,
    product_id: String,
) -> Result<[u8; 32], AuthorityError> {
    let sso = session.sso.as_ref().ok_or(AuthorityError::Disconnected)?;
    let lifecycle_epoch = host.grants.lifecycle().revision();
    let cache_key = (SsoSessionKey::from_session(sso), product_id.clone());
    if let Some(public_key) = host
        .grants
        .known_product_subtree(&host.sso.session_state(), session, cache_key.clone())
        .await
    {
        return Ok(public_key);
    }
    let public_key = host
        .holder
        .product_subtree_public_key(cx, &authority_session(session), product_id)
        .await?;
    if !host
        .grants
        .persist_product_subtree_if_current(
            &host.sso.session_state(),
            session,
            lifecycle_epoch,
            cache_key,
            public_key,
        )
        .await
    {
        return Err(AuthorityError::Disconnected);
    }
    Ok(public_key)
}

/// Return allocation outcomes from the paired signing host without retaining keys.
pub async fn remote_allocate_resources(
    host: &PairingHost,
    cx: &CallContext,
    session: &SessionInfo,
    product_id: String,
    request: latest::HostRequestResourceAllocationRequest,
) -> Result<Vec<SsoAllocationOutcome>, AuthorityError> {
    host.holder
        .allocate_resources(
            cx,
            &authority_session(session),
            product_id,
            request,
            OnExistingAllowancePolicy::Increase,
        )
        .await
}

/// Statement-store allowance key for the product, served from the cache
/// or allocated by the paired signing host.
pub async fn remote_statement_store_allowance_key(
    host: &PairingHost,
    cx: &CallContext,
    session: &SessionInfo,
    lifecycle_epoch: u64,
    product_id: String,
) -> Result<StatementStoreAllowanceKey, AuthorityError> {
    if let Some(cached) = host
        .grants
        .cached_statement_store_allowance_key(
            &host.sso.session_state(),
            session,
            lifecycle_epoch,
            &product_id,
        )
        .await?
    {
        return Ok(cached);
    }
    match host
        .remote_allowance_slot(
            cx,
            session,
            &product_id,
            latest::AllocatableResource::StatementStoreAllowance,
            OnExistingAllowancePolicy::Ignore,
        )
        .await?
    {
        SsoAllocatedResource::StatementStoreAllowance { slot_account_key } => {
            host.grants
                .cache_statement_store_allowance_key(
                    &host.sso.session_state(),
                    session,
                    lifecycle_epoch,
                    &product_id,
                    slot_account_key,
                )
                .await
        }
        other => Err(unexpected_resource("statement-store allowance", &other)),
    }
}

/// Bulletin allowance key for the product, served from the cache or
/// allocated by the paired signing host.
pub async fn remote_bulletin_allowance_key(
    host: &PairingHost,
    cx: &CallContext,
    session: &SessionInfo,
    lifecycle_epoch: u64,
    product_id: String,
) -> Result<BulletinAllowanceKey, AuthorityError> {
    if let Some(cached) = host
        .grants
        .cached_bulletin_allowance_key(
            &host.sso.session_state(),
            session,
            lifecycle_epoch,
            &product_id,
        )
        .await?
    {
        return Ok(cached);
    }
    host.allocate_bulletin_allowance_key(
        cx,
        session,
        lifecycle_epoch,
        product_id,
        OnExistingAllowancePolicy::Ignore,
    )
    .await
}

/// Evict the cached Bulletin allowance key and allocate a fresh one with
/// an increased allowance, so a stale or exhausted slot is never reused.
pub async fn remote_refresh_bulletin_allowance_key(
    host: &PairingHost,
    cx: &CallContext,
    session: &SessionInfo,
    lifecycle_epoch: u64,
    product_id: String,
) -> Result<BulletinAllowanceKey, AuthorityError> {
    host.grants
        .evict_bulletin_allowance_key(
            &host.sso.session_state(),
            session,
            lifecycle_epoch,
            &product_id,
        )
        .await?;
    host.allocate_bulletin_allowance_key(
        cx,
        session,
        lifecycle_epoch,
        product_id,
        OnExistingAllowancePolicy::Increase,
    )
    .await
}

fn allowance_name(resource: &latest::AllocatableResource) -> &'static str {
    match resource {
        latest::AllocatableResource::StatementStoreAllowance => "statement-store allowance",
        latest::AllocatableResource::BulletinAllowance => "bulletin allowance",
        _ => "resource",
    }
}

/// Reason for an allocation that came back as a different resource kind; names
/// only the kind so no key material reaches logs.
fn unexpected_resource(label: &str, resource: &SsoAllocatedResource) -> AuthorityError {
    AuthorityError::Unknown {
        reason: format!("Unexpected {label} response resource: {}", resource.kind()),
    }
}

impl From<SsoAllocationOutcome> for latest::AllocationOutcome {
    fn from(outcome: SsoAllocationOutcome) -> Self {
        match outcome {
            SsoAllocationOutcome::Allocated(_) => Self::Allocated,
            SsoAllocationOutcome::Rejected => Self::Rejected,
            SsoAllocationOutcome::NotAvailable => Self::NotAvailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpected_resource_reasons_include_only_safe_discriminants() {
        let resource = SsoAllocatedResource::AutoSigning {
            product_root_private_key: [0xA5; 64],
            ring_vrf_domain_entropy: [0x5A; 32],
        };

        let AuthorityError::Unknown { reason } =
            unexpected_resource("statement-store allowance", &resource)
        else {
            panic!("expected an unknown authority error");
        };

        assert_eq!(
            reason,
            "Unexpected statement-store allowance response resource: auto-signing"
        );
        assert!(!reason.contains("165, 165"));
    }
}
