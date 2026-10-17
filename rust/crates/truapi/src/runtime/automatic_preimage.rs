//! Explicit upload consent; independent of remote permissions and AutoSigning.
use crate::unix_time::current_unix_secs;

use parity_scale_codec::{Decode, Encode};

use crate::platform::{
    CoreStorageKey, PermissionAuthorizationStatus, PermissionDecision, PreimageSubmitReview,
    UserConfirmationReview,
};
use truapi::v01::GenericError;

use super::{AuthoritySession, ProductRuntimeHost};

const MAX_BYTES: u64 = 256 * 1024;
const MAX_UPLOADS: usize = 4;
const WINDOW_SECONDS: u64 = 60 * 60;
const CONSENT_CAS_ATTEMPTS: usize = 8;

#[derive(Encode, Decode)]
struct Consent {
    version: u8,
    status: PermissionAuthorizationStatus,
    revision: u64,
    attempts: [Option<u64>; MAX_UPLOADS],
}

impl Default for Consent {
    fn default() -> Self {
        Self {
            version: 1,
            status: PermissionAuthorizationStatus::NotDetermined,
            revision: 0,
            attempts: [None; MAX_UPLOADS],
        }
    }
}

impl Consent {
    fn reserve(&mut self, size: u64, now: u64) -> bool {
        if self.status != PermissionAuthorizationStatus::Authorized || size > MAX_BYTES {
            return false;
        }
        for attempt in &mut self.attempts {
            // Future timestamps remain charged when the host clock moves backwards.
            if attempt.is_some_and(|at| now.saturating_sub(at) >= WINDOW_SECONDS) {
                *attempt = None;
            }
        }
        let Some(slot) = self.attempts.iter_mut().find(|at| at.is_none()) else {
            return false;
        };
        *slot = Some(now);
        true
    }

    fn set_status(&mut self, status: PermissionAuthorizationStatus) -> Result<(), GenericError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| failure("Upload consent generation exhausted"))?;
        self.status = status;
        // Do not erase quota on revocation, reset, or re-grant.
        Ok(())
    }
}

/// Approval generation captured before allowance acquisition. Reservations are
/// charged before I/O, including failed or cancelled attempts; an internal
/// allowance retry remains the same attempt.
pub(super) struct UploadApproval {
    revision: u64,
}

impl ProductRuntimeHost {
    fn upload_key(&self, session: &AuthoritySession) -> CoreStorageKey {
        CoreStorageKey::AutomaticPreimageUploads {
            product_id: self.product.product_id.clone(),
            root_public_key: session.public_key,
            genesis_hash: self.services.bulletin.genesis_hash(),
        }
    }

    fn require_upload_session(&self, session: &AuthoritySession) -> Result<(), GenericError> {
        if self
            .authority
            .session_is_current(session, Some(&self.product.product_id))
        {
            Ok(())
        } else {
            Err(failure("Upload account changed or disconnected"))
        }
    }

    fn upload_admin_session(
        &self,
        root_public_key: [u8; 32],
    ) -> Result<AuthoritySession, GenericError> {
        self.authority
            .current_session()
            .filter(|session| session.public_key == root_public_key)
            .ok_or_else(|| failure("Upload account changed or disconnected"))
    }

    async fn read_upload_consent(
        &self,
        session: &AuthoritySession,
    ) -> Result<(Consent, Option<Vec<u8>>), GenericError> {
        let bytes = self
            .platform
            .read_core_storage(self.upload_key(session))
            .await?;
        self.require_upload_session(session)?;
        let Some(raw) = bytes.as_ref() else {
            return Ok((Consent::default(), None));
        };
        let mut input = raw.as_slice();
        let consent =
            Consent::decode(&mut input).map_err(|_| failure("Invalid automatic upload consent"))?;
        if consent.version != 1 || !input.is_empty() {
            return Err(failure("Invalid automatic upload consent"));
        }
        Ok((consent, bytes))
    }

    async fn compare_exchange_upload_consent(
        &self,
        session: &AuthoritySession,
        expected: Option<Vec<u8>>,
        consent: &Consent,
    ) -> Result<bool, GenericError> {
        self.require_upload_session(session)?;
        let changed = self
            .platform
            .compare_exchange_core_storage(
                self.upload_key(session),
                expected,
                consent.encode(),
                false,
            )
            .await?;
        self.require_upload_session(session)?;
        Ok(changed)
    }

    pub(super) async fn automatic_upload_status(
        &self,
        root_public_key: [u8; 32],
    ) -> Result<PermissionAuthorizationStatus, GenericError> {
        let session = self.upload_admin_session(root_public_key)?;
        let _gate = self.services.automatic_preimage_gate.lock().await;
        Ok(self.read_upload_consent(&session).await?.0.status)
    }

    pub(super) async fn set_automatic_upload_status(
        &self,
        root_public_key: [u8; 32],
        status: PermissionAuthorizationStatus,
    ) -> Result<(), GenericError> {
        let session = self.upload_admin_session(root_public_key)?;
        let _gate = self.services.automatic_preimage_gate.lock().await;
        for _ in 0..CONSENT_CAS_ATTEMPTS {
            let (mut consent, expected) = self.read_upload_consent(&session).await?;
            consent.set_status(status)?;
            if self
                .compare_exchange_upload_consent(&session, expected, &consent)
                .await?
            {
                return Ok(());
            }
        }
        Err(failure("Upload consent storage contention"))
    }

    pub(super) async fn approve_preimage_upload(
        &self,
        session: &AuthoritySession,
        size: u64,
    ) -> Result<UploadApproval, GenericError> {
        self.require_upload_session(session)?;
        let revision = {
            let _gate = self.services.automatic_preimage_gate.lock().await;
            let mut revision = None;
            for _ in 0..CONSENT_CAS_ATTEMPTS {
                let (mut consent, expected) = self.read_upload_consent(session).await?;
                if !consent.reserve(size, current_unix_secs()) {
                    revision = Some(consent.revision);
                    break;
                }
                // Separate host runtimes share the persisted quota, not this
                // runtime's gate. Only the successful CAS spends an attempt.
                if self
                    .compare_exchange_upload_consent(session, expected, &consent)
                    .await?
                {
                    return Ok(UploadApproval {
                        revision: consent.revision,
                    });
                }
            }
            revision.ok_or_else(|| failure("Upload consent storage contention"))?
        };
        // Never hold the storage gate across UI: revocation must remain usable
        // while this prompt is open. No trusted-product or AutoSigning shortcut.
        let decision = self
            .platform
            .confirm_permission(UserConfirmationReview::PreimageSubmit(
                PreimageSubmitReview {
                    size,
                    product_id: self.product.product_id.clone(),
                    root_public_key: session.public_key,
                    genesis_hash: self.services.bulletin.genesis_hash(),
                    automatic_max_bytes: MAX_BYTES,
                    automatic_max_uploads: MAX_UPLOADS as u32,
                    automatic_window_seconds: WINDOW_SECONDS as u32,
                },
            ))
            .await?;
        self.require_upload_session(session)?;
        let _gate = self.services.automatic_preimage_gate.lock().await;
        for _ in 0..CONSENT_CAS_ATTEMPTS {
            let (mut consent, expected) = self.read_upload_consent(session).await?;
            if consent.revision != revision {
                return Err(failure("Upload permission changed during confirmation"));
            }
            match decision {
                PermissionDecision::AllowOnce => return Ok(UploadApproval { revision }),
                PermissionDecision::AllowAlways => {
                    if consent.status != PermissionAuthorizationStatus::Authorized {
                        consent.set_status(PermissionAuthorizationStatus::Authorized)?;
                    }
                    // This explicit review can approve an oversized or over-budget
                    // upload once, but cannot reset or enlarge the automatic quota.
                    consent.reserve(size, current_unix_secs());
                }
                PermissionDecision::Deny => {
                    consent.set_status(PermissionAuthorizationStatus::Denied)?;
                }
            }
            if !self
                .compare_exchange_upload_consent(session, expected, &consent)
                .await?
            {
                // Another reservation may be retried; a changed decision above
                // rejects this stale answer rather than restoring old consent.
                continue;
            }
            return match decision {
                PermissionDecision::Deny => Err(failure("User rejected preimage submission")),
                _ => Ok(UploadApproval {
                    revision: consent.revision,
                }),
            };
        }
        Err(failure("Upload consent storage contention"))
    }

    /// Fence work that waited for a signing host. Once handed to Bulletin,
    /// an already submitted transaction cannot be withdrawn by revoking consent.
    pub(super) async fn validate_upload_approval(
        &self,
        session: &AuthoritySession,
        approval: &UploadApproval,
    ) -> Result<(), GenericError> {
        self.require_upload_session(session)?;
        let _gate = self.services.automatic_preimage_gate.lock().await;
        if self.read_upload_consent(session).await?.0.revision != approval.revision {
            return Err(failure("Upload permission was revoked or changed"));
        }
        Ok(())
    }
}

fn failure(reason: &str) -> GenericError {
    GenericError {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_window_expires_individual_attempts_and_retains_future_timestamps() {
        let mut consent = Consent::default();
        consent
            .set_status(PermissionAuthorizationStatus::Authorized)
            .unwrap();
        assert!(!consent.reserve(MAX_BYTES + 1, 100));
        for at in [100, 200, 300, 400] {
            assert!(consent.reserve(MAX_BYTES, at));
        }
        assert!(!consent.reserve(1, 50));
        assert!(!consent.reserve(1, 3699));
        assert!(consent.reserve(1, 3700));
        assert!(!consent.reserve(1, 3799));
        assert!(consent.reserve(1, 3800));
    }
}
