//! Account-access decisions and operation-review rules shared by local and paired callers.

use std::{
	collections::HashSet,
	sync::{Arc, Mutex},
};

use truapi::{CallContext, latest::GenericError};

use super::{
	ProductConnection,
	authority::{AccountCaller, AuthorityError},
};
use crate::{
	host_internal::permissions::{
		PermissionAuthority, authorization_status, cross_product_signature_key,
		set_authorization_status,
	},
	platform::{
		AccountAccessReview, CoreStorageKey, CreateTransactionReview, CrossProductSignatureKind,
		PermissionAuthorizationStatus, PermissionDecision, Platform, ProductId, SignPayloadReview,
		SignRawReview, UserConfirmationReview,
	},
};

/// One runtime's consent policy: one decision store, one prompt at a time, one set of rules.
pub struct ProductConsent {
	platform: Arc<dyn Platform>,
	permissions: Arc<PermissionAuthority>,
	permission_prompt: futures::lock::Mutex<()>,
	allowed_once: Mutex<HashSet<(String, String)>>,
}

impl ProductConsent {
	/// Store decisions through `platform`, the runtime-wide one.
	pub fn new(platform: Arc<dyn Platform>, permissions: Arc<PermissionAuthority>) -> Self {
		Self {
			platform,
			permissions,
			permission_prompt: futures::lock::Mutex::new(()),
			allowed_once: Mutex::default(),
		}
	}

	/// Whether `requester` may use `target`'s accounts, prompting through `prompt`
	/// when nothing is decided.
	///
	/// Allow once lasts until this runtime forgets it; only one prompt runs at a time,
	/// and a request that waited for it reuses the answer.
	/// `is_current` checks the captured activation under the permission mutation fence,
	/// before prompting and again before any answer is retained.
	pub async fn account_access(
		&self,
		requester: &ProductId,
		target: &ProductId,
		prompt: &dyn Platform,
		is_current: impl Fn() -> bool + Send,
	) -> Result<PermissionAuthorizationStatus, AccountAccessError> {
		self.account_access_with_trust(requester, target, prompt, true, true, is_current)
			.await
	}

	/// Disclose an account once without extending the answer to later disclosures.
	/// A reset or activation change supersedes an answer still pending.
	pub async fn account_disclosure(
		&self,
		requester: &ProductId,
		target: &ProductId,
		prompt: &dyn Platform,
		is_current: impl Fn() -> bool + Send,
	) -> Result<PermissionAuthorizationStatus, AccountAccessError> {
		self.account_access_with_trust(requester, target, prompt, true, false, is_current)
			.await
	}

	/// Migrated identity keys require a decision even from trusted products.
	pub async fn account_access_explicit(
		&self,
		requester: &ProductId,
		target: &ProductId,
		prompt: &dyn Platform,
		is_current: impl Fn() -> bool + Send,
	) -> Result<PermissionAuthorizationStatus, AccountAccessError> {
		self.account_access_with_trust(requester, target, prompt, false, true, is_current)
			.await
	}

	async fn account_access_with_trust(
		&self,
		requester: &ProductId,
		target: &ProductId,
		prompt: &dyn Platform,
		allow_trusted: bool,
		retain_once: bool,
		is_current: impl Fn() -> bool + Send,
	) -> Result<PermissionAuthorizationStatus, AccountAccessError> {
		if requester == target {
			return Ok(PermissionAuthorizationStatus::Authorized);
		}
		// Filed per product, the granularity a manifest grant uses, so a refused
		// product cannot keep access by respelling itself under a subname.
		let pair = (requester.bare_label().to_string(), target.bare_label().to_string());
		let scope = self.permissions.scope(requester.as_str());
		let revision = scope.revision();
		let _prompt = self.permission_prompt.lock().await;
		let mutation = scope.mutation.lock().await;
		scope.require_revision(revision).map_err(|_| AccountAccessError::Superseded)?;
		if !is_current() {
			return Err(AccountAccessError::Superseded);
		}
		if let Some(status) = self.decided(requester, target, retain_once).await? {
			return Ok(status);
		}
		if allow_trusted && requester.has_trusted_remote_permissions() {
			return Ok(PermissionAuthorizationStatus::Authorized);
		}
		drop(mutation);
		let decision = scope
			.prompt(
				revision,
				prompt.confirm_permission(UserConfirmationReview::AccountAccess(
					AccountAccessReview {
						requesting_product_id: requester.clone(),
						target_product_id: target.clone(),
					},
				)),
			)
			.await;
		let _mutation = scope.mutation.lock().await;
		// An aborted prompt is a withdrawn decision, not a platform failure.
		// Check under the same fence used to commit, including when the prompt failed.
		scope.require_revision(revision).map_err(|_| AccountAccessError::Superseded)?;
		if !is_current() {
			return Err(AccountAccessError::Superseded);
		}
		let decision = decision.map_err(AccountAccessError::Confirmation)?;
		let status = match decision {
			PermissionDecision::AllowOnce => {
				if retain_once {
					self.allowed_once.lock().expect("account access mutex poisoned").insert(pair);
				}
				return Ok(PermissionAuthorizationStatus::Authorized);
			},
			PermissionDecision::AllowAlways => PermissionAuthorizationStatus::Authorized,
			PermissionDecision::Deny => PermissionAuthorizationStatus::Denied,
		};
		set_authorization_status(
			self.platform.as_ref(),
			CoreStorageKey::product_account_access_authorization(requester, target),
			status,
		)
		.await
		.map_err(AccountAccessError::PermissionStorage)?;
		Ok(status)
	}

	/// Confirm an operation through `prompt` unless `caller` may skip its review.
	pub async fn review(
		&self,
		cx: &CallContext,
		caller: AccountCaller<'_>,
		prompt: &dyn Platform,
		review: UserConfirmationReview,
	) -> Result<(), AuthorityError> {
		if skips_review(caller, &review) {
			return Ok(());
		}
		let approved = super::until_cancelled(cx, prompt.confirm_user_action(review))
			.await?
			.map_err(AuthorityError::ConfirmationFailed)?;
		if approved { Ok(()) } else { Err(AuthorityError::Rejected) }
	}

	/// Confirm a signature by `connection`'s product with an account `owner` holds,
	/// on the product's own connection.
	///
	/// Another product's account is confirmed once per kind of signature and owning
	/// product, for as long as the connection's execution lasts. Signature prompts
	/// open one at a time, and a signature already approved never waits for one.
	pub async fn review_signature(
		&self,
		cx: &CallContext,
		connection: &ProductConnection,
		kind: CrossProductSignatureKind,
		owner: &ProductId,
		review: UserConfirmationReview,
	) -> Result<(), AuthorityError> {
		let product = &connection.product;
		let approvals = &connection.temporary_permissions;
		let prompt = connection.platform.as_ref();
		let caller = AccountCaller::Local { product };
		if *owner == product.product_id {
			return self.review(cx, caller, prompt, review).await;
		}
		let approval = cross_product_signature_key(&product.product_id, owner, kind);
		let scope = connection.permissions_service().scope();
		let revision = scope.revision();
		if approvals.authorize(&approval, false) {
			return Ok(());
		}
		let _prompt = super::until_cancelled(cx, approvals.signature_prompt()).await?;
		if approvals.authorize(&approval, false) {
			return Ok(());
		}
		self.review(cx, caller, prompt, review).await?;
		let _mutation = scope.mutation.lock().await;
		scope.require_revision(revision).map_err(AuthorityError::ConfirmationFailed)?;
		approvals.grant(approval);
		Ok(())
	}

	/// Forget every Allow once answer, as on lock or a wallet change.
	pub fn forget_allowed_once(&self) {
		self.allowed_once.lock().expect("account access mutex poisoned").clear();
	}

	/// Forget the Allow once answers `product_id` received, as on its reset or a
	/// change to its account access in settings.
	pub fn forget_allowed_once_for(&self, product_id: &ProductId) {
		let requester = product_id.bare_label();
		self.allowed_once
			.lock()
			.expect("account access mutex poisoned")
			.retain(|(granted, _)| granted != requester);
	}

	/// A stored decision wins over Allow once, so a change made in settings applies at once.
	async fn decided(
		&self,
		requester: &ProductId,
		target: &ProductId,
		retain_once: bool,
	) -> Result<Option<PermissionAuthorizationStatus>, AccountAccessError> {
		let stored = authorization_status(
			self.platform.as_ref(),
			CoreStorageKey::product_account_access_authorization(requester, target),
		)
		.await
		.map_err(AccountAccessError::PermissionStorage)?;
		if stored != PermissionAuthorizationStatus::NotDetermined {
			return Ok(Some(stored));
		}
		if !retain_once {
			return Ok(None);
		}
		let allowed_once = self
			.allowed_once
			.lock()
			.expect("account access mutex poisoned")
			.contains(&(requester.bare_label().to_string(), target.bare_label().to_string()));
		Ok(allowed_once.then_some(PermissionAuthorizationStatus::Authorized))
	}
}

/// Why an account-access decision could not be made.
#[derive(Debug, thiserror::Error)]
pub enum AccountAccessError {
	/// The wallet activation or product permission changed before the answer committed.
	#[error("account access decision was superseded")]
	Superseded,
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
	let first_party = product.product_id.has_trusted_remote_permissions();
	match review {
		UserConfirmationReview::SignPayload(SignPayloadReview::Product { .. }) |
		UserConfirmationReview::SignRaw(SignRawReview::Product { .. }) |
		UserConfirmationReview::CreateTransaction(CreateTransactionReview::Product { .. }) |
		UserConfirmationReview::ResourceAllocation(_) |
		UserConfirmationReview::ProductSubtree(_) |
		UserConfirmationReview::NftPurseDrain(_) => first_party,
		// Watermarked bytes cannot pass for a transaction's signing payload, so a
		// trusted product signs them the way it signs with its own accounts.
		UserConfirmationReview::SignRaw(SignRawReview::LegacyAccount {
			watermarked: true, ..
		}) => first_party,
		UserConfirmationReview::StatementStoreProductSign(review) => {
			first_party || review.account.dot_ns_identifier == product.product_id
		},
		UserConfirmationReview::SignVrf(review) => {
			first_party && review.request.account.dot_ns_identifier == product.product_id
		},
		_ => false,
	}
}

#[cfg(test)]
mod tests;
