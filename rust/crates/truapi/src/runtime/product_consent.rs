//! Account-access decisions and operation-review rules shared by local and paired callers.

use std::{
	collections::HashSet,
	sync::{Arc, Mutex},
};

use truapi::{CallContext, latest::GenericError};

use super::authority::{AccountCaller, AuthorityError};
use crate::{
	host_internal::{
		permissions::{account_access_status, set_account_access_status},
		product_manifest::bare_product_label,
	},
	platform::{
		AccountAccessReview, CreateTransactionReview, PermissionAuthorizationStatus,
		PermissionDecision, Platform, SignPayloadReview, SignRawReview, UserConfirmationReview,
		has_trusted_remote_permissions, normalize_product_identifier,
		normalizes_to_trusted_remote_permissions,
	},
};

/// One runtime's consent policy: one prompt channel, one store, one set of rules.
pub struct ProductConsent {
	platform: Arc<dyn Platform>,
	permission_prompt: futures::lock::Mutex<()>,
	allowed_once: Mutex<HashSet<(String, String)>>,
}

impl ProductConsent {
	/// Ask through `platform`, the runtime-wide channel, and store decisions there.
	pub fn new(platform: Arc<dyn Platform>) -> Self {
		Self {
			platform,
			permission_prompt: futures::lock::Mutex::new(()),
			allowed_once: Mutex::default(),
		}
	}

	/// Whether `requester` may use `target`'s accounts, prompting when nothing is decided.
	///
	/// Allow once lasts until this runtime forgets it; only one prompt runs at a time,
	/// and a request that waited for it reuses the answer.
	pub async fn account_access(
		&self,
		requester: &str,
		target: &str,
	) -> Result<PermissionAuthorizationStatus, AccountAccessError> {
		if requester == target || normalizes_to_trusted_remote_permissions(requester) {
			return Ok(PermissionAuthorizationStatus::Authorized);
		}
		// Filed per product, the granularity a manifest grant uses, so a refused
		// product cannot keep access by respelling itself under a subname.
		let pair =
			(bare_product_label(requester).to_string(), bare_product_label(target).to_string());
		if let Some(status) = self.decided(&pair).await? {
			return Ok(status);
		}
		let _prompt = self.permission_prompt.lock().await;
		if let Some(status) = self.decided(&pair).await? {
			return Ok(status);
		}
		let decision = self
			.platform
			.confirm_permission(UserConfirmationReview::AccountAccess(AccountAccessReview {
				requesting_product_id: requester.to_string(),
				target_product_id: target.to_string(),
			}))
			.await
			.map_err(AccountAccessError::Confirmation)?;
		let status = match decision {
			PermissionDecision::AllowOnce => {
				self.allowed_once.lock().expect("account access mutex poisoned").insert(pair);
				return Ok(PermissionAuthorizationStatus::Authorized);
			},
			PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
			PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
		};
		set_account_access_status(self.platform.as_ref(), &pair.0, &pair.1, status)
			.await
			.map_err(AccountAccessError::PermissionStorage)?;
		Ok(status)
	}

	/// Confirm an operation with the user unless `caller` may skip its review.
	pub async fn review(
		&self,
		cx: &CallContext,
		caller: AccountCaller<'_>,
		review: UserConfirmationReview,
	) -> Result<(), AuthorityError> {
		if skips_review(caller, &review) {
			return Ok(());
		}
		let approved = super::until_cancelled(cx, self.platform.confirm_user_action(review))
			.await?
			.map_err(AuthorityError::ConfirmationFailed)?;
		if approved { Ok(()) } else { Err(AuthorityError::Rejected) }
	}

	/// Forget every Allow once answer, as on lock or a wallet change.
	pub fn forget_allowed_once(&self) {
		self.allowed_once.lock().expect("account access mutex poisoned").clear();
	}

	/// Forget the Allow once answers `product_id` received, as on its reset.
	pub fn forget_allowed_once_for(&self, product_id: &str) {
		let requester = bare_product_label(product_id);
		self.allowed_once
			.lock()
			.expect("account access mutex poisoned")
			.retain(|(granted, _)| granted != requester);
	}

	/// A stored decision wins over Allow once, so a change made in settings applies at once.
	async fn decided(
		&self,
		(requester, target): &(String, String),
	) -> Result<Option<PermissionAuthorizationStatus>, AccountAccessError> {
		let stored = account_access_status(self.platform.as_ref(), requester, target)
			.await
			.map_err(AccountAccessError::PermissionStorage)?;
		if stored != PermissionAuthorizationStatus::NotDetermined {
			return Ok(Some(stored));
		}
		let allowed_once = self
			.allowed_once
			.lock()
			.expect("account access mutex poisoned")
			.contains(&(requester.clone(), target.clone()));
		Ok(allowed_once.then_some(PermissionAuthorizationStatus::Authorized))
	}
}

/// Why an account-access decision could not be made.
#[derive(Debug, thiserror::Error)]
pub enum AccountAccessError {
	/// The decision could not be read or written.
	#[error("permission storage failed: {0:?}")]
	PermissionStorage(GenericError),
	/// The prompt could not be shown or answered.
	#[error("account access confirmation failed: {0:?}")]
	Confirmation(GenericError),
}

/// Reviews a product bound by this host may skip; a paired caller never skips one.
fn skips_review(caller: AccountCaller<'_>, review: &UserConfirmationReview) -> bool {
	let AccountCaller::Local { product } = caller else {
		return false;
	};
	let first_party = has_trusted_remote_permissions(&product.product_id);
	match review {
		UserConfirmationReview::SignPayload(SignPayloadReview::Product { .. }) |
		UserConfirmationReview::SignRaw(SignRawReview::Product { .. }) |
		UserConfirmationReview::CreateTransaction(CreateTransactionReview::Product { .. }) |
		UserConfirmationReview::ResourceAllocation(_) |
		UserConfirmationReview::ProductSubtree(_) => first_party,
		UserConfirmationReview::StatementStoreProductSign(review) => {
			first_party || review.account.dot_ns_identifier == product.product_id
		},
		UserConfirmationReview::SignVrf(review) => {
			first_party &&
				normalize_product_identifier(&review.request.account.dot_ns_identifier)
					.is_ok_and(|owner| owner == product.product_id)
		},
		_ => false,
	}
}
