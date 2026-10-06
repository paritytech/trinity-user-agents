use std::sync::{Arc, Weak};

use futures::future::{AbortHandle, AbortRegistration};
use futures::lock::{Mutex, MutexGuard};

use super::TemporaryPermissions;
use crate::latest::RemotePermission;
use crate::platform::{CoreStorageKey, PermissionAuthorizationRequest, remote_domain_candidates};

/// Orders saved decisions against active reviews and execution-local grants.
#[derive(Default)]
pub struct PermissionCoordinator {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    reviews: Vec<Weak<PermissionReview>>,
    executions: Vec<Weak<TemporaryPermissions>>,
}

/// The exact permission question whose outstanding work can be withdrawn.
pub struct PermissionReview {
    key: CoreStorageKey,
    abort: AbortHandle,
}

/// Keeps authorization use and decision publication ordered with settings.
pub struct PermissionCommit<'a> {
    _state: MutexGuard<'a, State>,
}

impl PermissionCoordinator {
    /// Register before reading an answer or waiting for a prompt.
    pub async fn review(
        &self,
        key: CoreStorageKey,
        permissions: Option<&Arc<TemporaryPermissions>>,
    ) -> (Arc<PermissionReview>, AbortRegistration) {
        let (abort, registration) = AbortHandle::new_pair();
        let review = Arc::new(PermissionReview { key, abort });
        let mut state = self.state.lock().await;
        state.reviews.retain(|review| review.strong_count() != 0);
        state.reviews.push(Arc::downgrade(&review));
        if let Some(permissions) = permissions {
            state
                .executions
                .retain(|execution| execution.strong_count() != 0);
            let execution = Arc::downgrade(permissions);
            if !state
                .executions
                .iter()
                .any(|existing| existing.ptr_eq(&execution))
            {
                state.executions.push(execution);
            }
        }
        (review, registration)
    }

    /// A withdrawn review cannot consume or publish authorization.
    pub async fn require(&self, review: &PermissionReview) -> Option<PermissionCommit<'_>> {
        let state = self.state.lock().await;
        (!review.abort.is_aborted()).then_some(PermissionCommit { _state: state })
    }

    /// Withdraw affected work before editing its saved answer.
    pub async fn change(&self, key: &CoreStorageKey) -> PermissionCommit<'_> {
        let mut state = self.state.lock().await;
        state.reviews.retain(|review| {
            let Some(review) = review.upgrade() else {
                return false;
            };
            if overlaps(key, &review.key) {
                review.abort.abort();
            }
            true
        });
        state.executions.retain(|execution| {
            let Some(execution) = execution.upgrade() else {
                return false;
            };
            execution.revoke_matching(key);
            true
        });
        PermissionCommit { _state: state }
    }
}

/// Whether a saved change can affect an outstanding permission question.
pub fn overlaps(left: &CoreStorageKey, right: &CoreStorageKey) -> bool {
    let (
        CoreStorageKey::PermissionAuthorization {
            product_id: left_product,
            request: left,
        },
        CoreStorageKey::PermissionAuthorization {
            product_id: right_product,
            request: right,
        },
    ) = (left, right)
    else {
        return false;
    };
    if left_product != right_product {
        return false;
    }
    match (left, right) {
        (
            PermissionAuthorizationRequest::Remote(left),
            PermissionAuthorizationRequest::Remote(right),
        ) => match (&left.permission, &right.permission) {
            (
                RemotePermission::Remote { domains: left },
                RemotePermission::Remote { domains: right },
            ) => left.iter().any(|left| {
                right.iter().any(|right| {
                    remote_domain_candidates(left).contains(right)
                        || remote_domain_candidates(right).contains(left)
                })
            }),
            _ => left == right,
        },
        _ => left == right,
    }
}
