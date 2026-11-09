//! Runtime authorization and lifetime for protected native validator custody.

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use futures::{FutureExt, StreamExt};
use crate::{CallContext, CallError, CancellationToken, v01};
use crate::versioned::{IntoLatest, validator as wire};
use crate::platform::{PermissionAuthorizationStatus, ValidatorPlatform};
use super::{AuthoritySession, ProductRuntimeHost};

#[derive(Default)]
pub(super) struct ValidatorState {
    owner: Option<ValidatorOwner>,
}

struct ValidatorOwner {
    authority: v01::ValidatorAuthority,
    session: AuthoritySession,
    genesis: [u8; 32],
    handle: String,
    parameters: [u8; 32],
    authoring: bool,
    full_duties: bool,
    revoked: Arc<AtomicBool>,
    stop: CancellationToken,
}

impl Drop for ValidatorOwner {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[derive(Clone)]
pub(super) struct ValidatorRevocation {
    account: [u8; 32],
    genesis: [u8; 32],
    revoked: Arc<AtomicBool>,
    platform: Arc<dyn ValidatorPlatform>,
}

impl super::RuntimeServices {
    pub(super) fn revoke_validator_permission(&self, product: &str, permission: &v01::RemotePermission) {
        let v01::RemotePermission::Validator { genesis, account, execution } = permission else {
            return
        };
        let owner = self.validator_owners.lock()
            .get(&(product.to_owned(), execution.clone()))
            .filter(|owner| owner.account == *account && owner.genesis == *genesis)
            .cloned();
        if let Some(owner) = owner {
            owner.revoked.store(true, Ordering::Release);
            owner.platform.validator_revoke(execution.clone());
        }
    }
}

/// Dropping an admitted operation is ambiguous, never an ownership release.
struct Admission<'a> {
    platform: &'a dyn ValidatorPlatform,
    execution: String,
    revoked: Arc<AtomicBool>,
    armed: bool,
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.revoked.store(true, Ordering::Release);
            self.platform.validator_revoke(self.execution.clone());
        }
    }
}

fn decimal(value: &str) -> bool {
    !value.is_empty() && value.len() <= 20
        && (value == "0" || !value.starts_with('0'))
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok()
}

/// Process-random high half of the trusted execution id. A bare per-process
/// counter repeats across host restarts while per-execution permission
/// decisions are stored durably, so the id must not repeat; it stays a
/// canonical unsigned decimal as `ValidatorAuthority` documents.
fn execution_epoch() -> u64 {
    static EPOCH: std::sync::LazyLock<u64> = std::sync::LazyLock::new(|| {
        let mut bytes = [0u8; 4];
        getrandom::getrandom(&mut bytes).expect("host entropy");
        u64::from(u32::from_le_bytes(bytes)) << 32
    });
    *EPOCH
}

impl ProductRuntimeHost {
    pub(super) fn execution_id(&self) -> String {
        (execution_epoch() | (self.core_instance & 0xffff_ffff)).to_string()
    }
}

fn recovery_valid(recovery: &v01::ValidatorRecoveryRef) -> bool {
    recovery.length > 0 && recovery.length <= v01::VALIDATOR_MAX_RECOVERY_BYTES
}

fn full_duty_valid(duty: &v01::ValidatorFullDuty) -> bool {
    if let v01::ValidatorFullDuty::Announcement { reports, .. } = duty {
        // Enrolled core bounds are checked by native custody. Preserve protocol
        // ordering (the initial audit announcement uses shuffled core order).
        if reports.is_empty() || reports.len() > u16::MAX as usize { return false }
        let mut seen = [0u64; 1024];
        reports.iter().all(|report| {
            let core = usize::from(report.core);
            let mask = 1u64 << (core % 64);
            let word = &mut seen[core / 64];
            let fresh = *word & mask == 0;
            *word |= mask;
            fresh
        })
    } else {
        true
    }
}

impl ProductRuntimeHost {
    fn validator_adapter(&self) -> Result<&dyn ValidatorPlatform, v01::HostValidatorError> {
        self.validator_platform.as_deref().ok_or(v01::HostValidatorError::Unsupported)
    }

    fn validator_live(&self, owner: &ValidatorOwner, cx: &CallContext) -> Result<(), v01::HostValidatorError> {
        if cx.cancel().is_cancelled() || owner.revoked.load(Ordering::Acquire)
            || self.authority.current_session().as_ref() != Some(&owner.session)
        {
            owner.revoked.store(true, Ordering::Release);
            if let Some(platform) = &self.validator_platform {
                platform.validator_revoke(owner.authority.execution_id.clone());
            }
            return Err(v01::HostValidatorError::Closed)
        }
        Ok(())
    }

    fn validator_authoring(&self, owner: &ValidatorOwner, parameters: &[u8]) -> Result<(), v01::HostValidatorError> {
        if !owner.authoring { return Err(v01::HostValidatorError::Unsupported) }
        if parameters.is_empty() || parameters.len() > v01::VALIDATOR_MAX_PARAMETERS_BYTES {
            return Err(v01::HostValidatorError::Limit)
        }
        if sp_crypto_hashing::blake2_256(parameters) != owner.parameters {
            return Err(v01::HostValidatorError::NotGranted)
        }
        Ok(())
    }

    fn validator_full(&self, owner: &ValidatorOwner, parameters: &[u8]) -> Result<(), v01::HostValidatorError> {
        if !owner.full_duties { return Err(v01::HostValidatorError::NotGranted) }
        self.validator_authoring(owner, parameters)
    }

    async fn validator_permission(&self, owner: &ValidatorOwner) -> Result<(), v01::HostValidatorError> {
        let request = v01::RemotePermissionRequest {
            permission: v01::RemotePermission::Validator {
                genesis: owner.genesis,
                account: owner.authority.account,
                execution: owner.authority.execution_id.clone(),
            },
        };
        match self.permissions_service().check_or_prompt_remote(request).await {
            Ok(PermissionAuthorizationStatus::Authorized) => Ok(()),
            Ok(_) => {
                owner.revoked.store(true, Ordering::Release);
                self.validator_adapter()?.validator_revoke(owner.authority.execution_id.clone());
                Err(v01::HostValidatorError::NotGranted)
            },
            Err(_) => Err(v01::HostValidatorError::Unavailable),
        }
    }

    async fn validator_call<T>(
        &self,
        owner: &ValidatorOwner,
        cx: &CallContext,
        operation: impl Future<Output = Result<T, v01::HostValidatorError>>,
    ) -> Result<T, v01::HostValidatorError> {
        self.validator_live(owner, cx)?;
        self.validator_permission(owner).await?;
        self.validator_live(owner, cx)?;
        let mut admission = Admission {
            platform: self.validator_adapter()?,
            execution: owner.authority.execution_id.clone(),
            revoked: owner.revoked.clone(),
            armed: true,
        };
        let operation = operation.fuse();
        let cancelled = cx.cancel().cancelled().fuse();
        futures::pin_mut!(operation, cancelled);
        let result = futures::select_biased! {
            _ = cancelled => return Err(v01::HostValidatorError::Closed),
            result = operation => result,
        };
        self.validator_live(owner, cx)?;
        if matches!(&result, Err(v01::HostValidatorError::Unknown { .. })) {
            // A callback may have acted before its response failed to decode.
            // Keep admission armed so disposal fences the owner; hide diagnostics.
            return Err(v01::HostValidatorError::Poisoned)
        }
        admission.armed = false;
        result
    }

    fn watch_validator_owner(&self, owner: &ValidatorOwner) {
        let mut changes = self.authority.session_state().subscribe();
        let authority = self.authority.clone();
        let session = owner.session.clone();
        let platform = self.validator_platform.as_ref().expect("adapter checked").clone();
        let execution = owner.authority.execution_id.clone();
        let revoked = owner.revoked.clone();
        let stop = owner.stop.clone();
        (self.services.spawner)(Box::pin(async move {
            loop {
                let stopped = stop.cancelled().fuse();
                let changed = changes.next().fuse();
                futures::pin_mut!(stopped, changed);
                futures::select_biased! {
                    _ = stopped => return,
                    item = changed => {
                        if item.is_none() || authority.current_session().as_ref() != Some(&session) {
                            revoked.store(true, Ordering::Release);
                            platform.validator_revoke(execution);
                            return
                        }
                    },
                }
            }
        }));
    }
}

#[crate::async_trait]
impl crate::api::Validator for ProductRuntimeHost {
    async fn open(&self, cx: &CallContext, request: wire::HostValidatorOpenRequest)
        -> Result<wire::HostValidatorOpenResponse, CallError<wire::HostValidatorOpenError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let mut state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            match &request {
                v01::HostValidatorOpenRequest::Acquire { genesis } => {
                    if state.owner.is_some() { return Err(v01::HostValidatorError::Busy) }
                    let session = self.authority.current_session().ok_or(v01::HostValidatorError::NotGranted)?;
                    let mut owner = ValidatorOwner {
                        authority: v01::ValidatorAuthority {
                            product_id: self.product.product_id.clone(),
                            account: session.public_key,
                            execution_id: self.execution_id(),
                        },
                        session,
                        genesis: *genesis,
                        handle: String::new(),
                        parameters: [0; 32],
                        authoring: false,
                        full_duties: false,
                        revoked: self.validator_revoked.clone(),
                        stop: CancellationToken::default(),
                    };
                    self.services.validator_owners.lock().insert(
                        (self.product.product_id.clone(), owner.authority.execution_id.clone()),
                        ValidatorRevocation {
                            account: owner.authority.account,
                            genesis: owner.genesis,
                            revoked: owner.revoked.clone(),
                            platform: self.validator_platform.as_ref().expect("adapter checked").clone(),
                        },
                    );
                    self.watch_validator_owner(&owner);
                    let response = self.validator_call(&owner, cx, platform.validator_open(&owner.authority, request)).await?;
                    let v01::HostValidatorOpenResponse::Acquired { handle, genesis, parameters, authoring, full_duties, generation, revision, unix_ms, max_recovery_bytes, max_chunk_bytes, max_storage_bytes, .. } = &response else {
                        platform.validator_revoke(owner.authority.execution_id.clone());
                        return Err(v01::HostValidatorError::Poisoned)
                    };
                    if handle.is_empty() || handle.len() > 256 || *genesis != owner.genesis
                        || !decimal(generation) || !decimal(revision) || !decimal(max_storage_bytes)
                        || !decimal(unix_ms)
                        || (*full_duties && !*authoring)
                        || *max_recovery_bytes > v01::VALIDATOR_MAX_RECOVERY_BYTES
                        || *max_chunk_bytes > v01::VALIDATOR_MAX_CHUNK_BYTES
                    {
                        platform.validator_revoke(owner.authority.execution_id.clone());
                        return Err(v01::HostValidatorError::Poisoned)
                    }
                    owner.handle = handle.clone();
                    owner.parameters = *parameters;
                    owner.authoring = *authoring;
                    owner.full_duties = *full_duties;
                    state.owner = Some(owner);
                    Ok(response)
                },
                v01::HostValidatorOpenRequest::Read { handle, recovery, offset, max_bytes }
                | v01::HostValidatorOpenRequest::ArchiveRead { handle, recovery, offset, max_bytes } => {
                    if !recovery_valid(recovery) || *max_bytes == 0 || *max_bytes > v01::VALIDATOR_MAX_CHUNK_BYTES
                        || *offset > recovery.length { return Err(v01::HostValidatorError::Invalid) }
                    let owner = state.owner.as_ref().filter(|owner| &owner.handle == handle).ok_or(v01::HostValidatorError::Closed)?;
                    self.validator_call(owner, cx, platform.validator_open(&owner.authority, request)).await
                },
                v01::HostValidatorOpenRequest::ArchiveInfo { handle, .. } => {
                    let owner = state.owner.as_ref().filter(|owner| &owner.handle == handle).ok_or(v01::HostValidatorError::Closed)?;
                    self.validator_call(owner, cx, platform.validator_open(&owner.authority, request)).await
                },
                v01::HostValidatorOpenRequest::LookupIntent { handle, revision, stage, round, .. } => {
                    if !decimal(revision) || !decimal(round) || *stage > 2 { return Err(v01::HostValidatorError::Invalid) }
                    let owner = state.owner.as_ref().filter(|owner| &owner.handle == handle).ok_or(v01::HostValidatorError::Closed)?;
                    self.validator_call(owner, cx, platform.validator_open(&owner.authority, request)).await
                },
                v01::HostValidatorOpenRequest::LookupFull { handle, revision, duty, .. } => {
                    if !decimal(revision) || !full_duty_valid(duty) { return Err(v01::HostValidatorError::Invalid) }
                    let owner = state.owner.as_ref().filter(|owner| &owner.handle == handle).ok_or(v01::HostValidatorError::Closed)?;
                    if !owner.full_duties { return Err(v01::HostValidatorError::NotGranted) }
                    self.validator_call(owner, cx, platform.validator_open(&owner.authority, request)).await
                },
                v01::HostValidatorOpenRequest::Ready { handle, revision, .. }
                | v01::HostValidatorOpenRequest::Retained { handle, revision, .. }
                | v01::HostValidatorOpenRequest::LookupSeal { handle, revision, .. } => {
                    if !decimal(revision) { return Err(v01::HostValidatorError::Invalid) }
                    let owner = state.owner.as_ref().filter(|owner| &owner.handle == handle).ok_or(v01::HostValidatorError::Closed)?;
                    self.validator_call(owner, cx, platform.validator_open(&owner.authority, request)).await
                },
            }
        }.await;
        result.map(wire::HostValidatorOpenResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorOpenError::V1(error)))
    }

    async fn commit(&self, cx: &CallContext, request: wire::HostValidatorCommitRequest)
        -> Result<wire::HostValidatorCommitResponse, CallError<wire::HostValidatorCommitError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            let handle = match &request {
                v01::HostValidatorCommitRequest::Begin { handle, recovery }
                | v01::HostValidatorCommitRequest::ArchiveBegin { handle, recovery } => {
                    if !recovery_valid(recovery) { return Err(v01::HostValidatorError::Invalid) }
                    handle
                },
                v01::HostValidatorCommitRequest::Chunk { handle, bytes, .. }
                | v01::HostValidatorCommitRequest::ArchiveChunk { handle, bytes, .. } => {
                    if bytes.is_empty() || bytes.len() > v01::VALIDATOR_MAX_CHUNK_BYTES as usize { return Err(v01::HostValidatorError::Limit) }
                    handle
                },
                v01::HostValidatorCommitRequest::Finish { handle }
                | v01::HostValidatorCommitRequest::ArchiveFinish { handle } => handle,
                v01::HostValidatorCommitRequest::Checkpoint { handle, revision, recovery, .. }
                | v01::HostValidatorCommitRequest::Supersede { handle, revision, recovery, .. } => {
                    if !decimal(revision) || !recovery_valid(recovery) { return Err(v01::HostValidatorError::Invalid) }
                    handle
                },
            };
            let owner = state.owner.as_ref().filter(|owner| &owner.handle == handle).ok_or(v01::HostValidatorError::Closed)?;
            self.validator_call(owner, cx, platform.validator_commit(&owner.authority, request)).await
        }.await;
        result.map(wire::HostValidatorCommitResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorCommitError::V1(error)))
    }

    async fn sign(&self, cx: &CallContext, request: wire::HostValidatorSignRequest)
        -> Result<wire::HostValidatorSignResponse, CallError<wire::HostValidatorSignError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            if request.duty.stage > 2 { return Err(v01::HostValidatorError::Unsupported) }
            if !decimal(&request.revision) || !decimal(&request.duty.round)
                || !recovery_valid(&request.recovery) { return Err(v01::HostValidatorError::Invalid) }
            let owner = state.owner.as_ref().filter(|owner| owner.handle == request.handle).ok_or(v01::HostValidatorError::Closed)?;
            self.validator_call(owner, cx, platform.validator_sign(&owner.authority, request)).await
        }.await;
        result.map(wire::HostValidatorSignResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorSignError::V1(error)))
    }

    async fn vrf_output(&self, cx: &CallContext, request: wire::HostValidatorVrfOutputRequest)
        -> Result<wire::HostValidatorVrfOutputResponse, CallError<wire::HostValidatorVrfOutputError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            let owner = state.owner.as_ref().filter(|owner| owner.handle == request.handle).ok_or(v01::HostValidatorError::Closed)?;
            self.validator_authoring(owner, &request.parameters)?;
            self.validator_call(owner, cx, platform.validator_vrf_output(&owner.authority, request)).await
        }.await;
        result.map(wire::HostValidatorVrfOutputResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorVrfOutputError::V1(error)))
    }

    async fn ticket_prove(&self, cx: &CallContext, request: wire::HostValidatorTicketProveRequest)
        -> Result<wire::HostValidatorTicketProveResponse, CallError<wire::HostValidatorTicketProveError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            let owner = state.owner.as_ref().filter(|owner| owner.handle == request.handle).ok_or(v01::HostValidatorError::Closed)?;
            self.validator_authoring(owner, &request.parameters)?;
            if request.ring.is_empty() || request.ring.len() > v01::VALIDATOR_MAX_RING
                || usize::from(request.index) >= request.ring.len() { return Err(v01::HostValidatorError::Limit) }
            self.validator_call(owner, cx, platform.validator_ticket_prove(&owner.authority, request)).await
        }.await;
        result.map(wire::HostValidatorTicketProveResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorTicketProveError::V1(error)))
    }

    async fn entropy_source(&self, cx: &CallContext, request: wire::HostValidatorEntropySourceRequest)
        -> Result<wire::HostValidatorEntropySourceResponse, CallError<wire::HostValidatorEntropySourceError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            let owner = state.owner.as_ref().filter(|owner| owner.handle == request.handle).ok_or(v01::HostValidatorError::Closed)?;
            self.validator_authoring(owner, &request.parameters)?;
            self.validator_call(owner, cx, platform.validator_entropy_source(&owner.authority, request)).await
        }.await;
        result.map(wire::HostValidatorEntropySourceResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorEntropySourceError::V1(error)))
    }

    async fn seal(&self, cx: &CallContext, request: wire::HostValidatorSealRequest)
        -> Result<wire::HostValidatorSealResponse, CallError<wire::HostValidatorSealError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            let owner = state.owner.as_ref().filter(|owner| owner.handle == request.handle).ok_or(v01::HostValidatorError::Closed)?;
            self.validator_authoring(owner, &request.parameters)?;
            if !decimal(&request.revision) || !recovery_valid(&request.recovery) {
                return Err(v01::HostValidatorError::Invalid)
            }
            if request.payload.is_empty() || request.payload.len() > v01::VALIDATOR_MAX_CHUNK_BYTES as usize {
                return Err(v01::HostValidatorError::Limit)
            }
            self.validator_call(owner, cx, platform.validator_seal(&owner.authority, request)).await
        }.await;
        result.map(wire::HostValidatorSealResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorSealError::V1(error)))
    }

    async fn full_sign(&self, cx: &CallContext, request: wire::HostValidatorFullSignRequest)
        -> Result<wire::HostValidatorFullSignResponse, CallError<wire::HostValidatorFullSignError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            let owner = state.owner.as_ref().filter(|owner| owner.handle == request.handle).ok_or(v01::HostValidatorError::Closed)?;
            self.validator_full(owner, &request.parameters)?;
            if !decimal(&request.revision) || !recovery_valid(&request.recovery) || !full_duty_valid(&request.duty) {
                return Err(v01::HostValidatorError::Invalid)
            }
            self.validator_call(owner, cx, platform.validator_full_sign(&owner.authority, request)).await
        }.await;
        result.map(wire::HostValidatorFullSignResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorFullSignError::V1(error)))
    }

    async fn audit_prove(&self, cx: &CallContext, request: wire::HostValidatorAuditProveRequest)
        -> Result<wire::HostValidatorAuditProveResponse, CallError<wire::HostValidatorAuditProveError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            let owner = state.owner.as_ref().filter(|owner| owner.handle == request.handle).ok_or(v01::HostValidatorError::Closed)?;
            self.validator_full(owner, &request.parameters)?;
            self.validator_call(owner, cx, platform.validator_audit_prove(&owner.authority, request)).await
        }.await;
        result.map(wire::HostValidatorAuditProveResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorAuditProveError::V1(error)))
    }

    async fn close(&self, cx: &CallContext, request: wire::HostValidatorCloseRequest)
        -> Result<wire::HostValidatorCloseResponse, CallError<wire::HostValidatorCloseError>>
    {
        let result = async {
            let platform = self.validator_adapter()?;
            let mut state = self.validator_state.try_lock().ok_or(v01::HostValidatorError::Busy)?;
            let request = request.into_latest();
            let owner = state.owner.as_ref().filter(|owner| owner.handle == request.handle).ok_or(v01::HostValidatorError::Closed)?;
            // Closing remains possible after revocation; it grants no new signing work.
            let mut admission = Admission {
                platform,
                execution: owner.authority.execution_id.clone(),
                revoked: owner.revoked.clone(),
                armed: true,
            };
            owner.revoked.store(true, Ordering::Release);
            let authority = owner.authority.clone();
            let close = platform.validator_close(&authority, request).fuse();
            let cancelled = cx.cancel().cancelled().fuse();
            futures::pin_mut!(close, cancelled);
            futures::select_biased! {
                _ = cancelled => return Err(v01::HostValidatorError::Closed),
                result = close => result.map_err(|error| match error {
                    v01::HostValidatorError::Unknown { .. } => v01::HostValidatorError::Poisoned,
                    error => error,
                })?,
            }
            admission.armed = false;
            state.owner = None;
            Ok(())
        }.await;
        result.map(|()| wire::HostValidatorCloseResponse::V1).map_err(|error| CallError::Domain(wire::HostValidatorCloseError::V1(error)))
    }
}

#[cfg(test)]
mod tests {
    use super::decimal;

    #[test]
    fn decimal_u64_has_one_lossless_spelling() {
        for value in ["0", "9007199254740993", "18446744073709551615"] {
            assert!(decimal(value), "{value}");
        }
        for value in ["", "00", "01", "+1", "-1", "1.0", "1e3", " 1", "18446744073709551616"] {
            assert!(!decimal(value), "{value}");
        }
    }

    #[test]
    fn full_announcements_keep_shuffle_order_but_refuse_duplicate_cores() {
        use crate::v01::{ValidatorAuditReport, ValidatorFullDuty};
        let announcement = |reports| ValidatorFullDuty::Announcement { anchor: [0; 32], tranche: 0, reports };
        assert!(!super::full_duty_valid(&announcement(vec![])));
        let first = ValidatorAuditReport { core: 7, report: [1; 32] };
        let second = ValidatorAuditReport { core: 2, report: [2; 32] };
        assert!(super::full_duty_valid(&announcement(vec![first.clone(), second])));
        assert!(!super::full_duty_valid(&announcement(vec![first.clone(), first])));
    }
}

#[cfg(test)]
#[path = "validator_tests.rs"]
mod behavioral_tests;
