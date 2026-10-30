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
		account_access_status, cross_product_signature_key, set_account_access_status,
	},
	platform::{
		AccountAccessReview, CreateTransactionReview, CrossProductSignatureKind,
		PermissionAuthorizationStatus, PermissionDecision, Platform, ProductId, SignPayloadReview,
		SignRawReview, UserConfirmationReview,
	},
};

/// One runtime's consent policy: one decision store, one prompt at a time, one set of rules.
pub struct ProductConsent {
	platform: Arc<dyn Platform>,
	permission_prompt: futures::lock::Mutex<()>,
	allowed_once: Mutex<HashSet<(String, String)>>,
}

impl ProductConsent {
	/// Store decisions through `platform`, the runtime-wide one.
	pub fn new(platform: Arc<dyn Platform>) -> Self {
		Self {
			platform,
			permission_prompt: futures::lock::Mutex::new(()),
			allowed_once: Mutex::default(),
		}
	}

	/// Whether `requester` may use `target`'s accounts, prompting through `prompt`
	/// when nothing is decided.
	///
	/// Allow once lasts until this runtime forgets it; only one prompt runs at a time,
	/// and a request that waited for it reuses the answer.
	pub async fn account_access(
		&self,
		requester: &ProductId,
		target: &ProductId,
		prompt: &dyn Platform,
	) -> Result<PermissionAuthorizationStatus, AccountAccessError> {
		if requester == target || requester.has_trusted_remote_permissions() {
			return Ok(PermissionAuthorizationStatus::Authorized);
		}
		// Filed per product, the granularity a manifest grant uses, so a refused
		// product cannot keep access by respelling itself under a subname.
		let pair = (requester.bare_label().to_string(), target.bare_label().to_string());
		if let Some(status) = self.decided(&pair).await? {
			return Ok(status);
		}
		let _prompt = self.permission_prompt.lock().await;
		if let Some(status) = self.decided(&pair).await? {
			return Ok(status);
		}
		let decision = prompt
			.confirm_permission(UserConfirmationReview::AccountAccess(AccountAccessReview {
				requesting_product_id: requester.clone(),
				target_product_id: target.clone(),
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
		if approvals.authorize(&approval, false) {
			return Ok(());
		}
		let _prompt = super::until_cancelled(cx, approvals.signature_prompt()).await?;
		if approvals.authorize(&approval, false) {
			return Ok(());
		}
		self.review(cx, caller, prompt, review).await?;
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
mod tests {
	use super::*;
	use crate::test_support::StubPlatform;
	use PermissionAuthorizationStatus::{Authorized, Denied};

	fn consent_answering(
		decisions: impl IntoIterator<Item = PermissionDecision>,
	) -> (Arc<StubPlatform>, ProductConsent) {
		let platform = Arc::new(StubPlatform {
			permission_confirmation_decisions: Mutex::new(decisions.into_iter().collect()),
			..StubPlatform::default()
		});
		(platform.clone(), ProductConsent::new(platform))
	}

	fn access(consent: &ProductConsent, requester: &str) -> PermissionAuthorizationStatus {
		futures::executor::block_on(consent.account_access(
			&ProductId::for_test(requester),
			&ProductId::for_test("peopl.dot"),
			consent.platform.as_ref(),
		))
		.expect("the stub answers every prompt")
	}

	fn prompts(platform: &StubPlatform) -> usize {
		platform.account_access_reviews.lock().unwrap().len()
	}

	/// Resetting a product drops what it was allowed once, not what others were.
	#[test]
	fn a_product_reset_forgets_only_that_products_allow_once() {
		let (platform, consent) = consent_answering([
			PermissionDecision::AllowOnce,
			PermissionDecision::AllowOnce,
			PermissionDecision::Deny,
		]);
		access(&consent, "myapp.dot");
		access(&consent, "other.dot");
		consent.forget_allowed_once_for(&ProductId::for_test("myapp.dot"));
		assert_eq!(
			(access(&consent, "other.dot"), access(&consent, "myapp.dot"), prompts(&platform)),
			(Authorized, Denied, 3)
		);
	}

	/// A decision changed in settings applies at once, even over Allow once.
	#[test]
	fn a_stored_decision_overrides_allow_once() {
		let (platform, consent) = consent_answering([PermissionDecision::AllowOnce]);
		access(&consent, "myapp.dot");
		futures::executor::block_on(set_account_access_status(
			platform.as_ref(),
			"myapp",
			"peopl",
			Denied,
		))
		.unwrap();
		assert_eq!((access(&consent, "myapp.dot"), prompts(&platform)), (Denied, 1));
	}

	/// Two calls racing for the same decision raise one prompt, and the
	/// second reuses its answer instead of asking again.
	#[test]
	fn a_request_waiting_for_the_prompt_reuses_its_answer() {
		let (release, gate) = futures::channel::oneshot::channel();
		let platform = Arc::new(StubPlatform {
			permission_confirmation_decisions: Mutex::new(
				[PermissionDecision::AllowOnce, PermissionDecision::Deny].into(),
			),
			account_access_confirmation_gate: Mutex::new(Some(gate)),
			..StubPlatform::default()
		});
		let consent = ProductConsent::new(platform.clone());
		let (requester, target) =
			(ProductId::for_test("myapp.dot"), ProductId::for_test("peopl.dot"));
		let (first, second, ()) = futures::executor::block_on(async {
			futures::join!(
				consent.account_access(&requester, &target, platform.as_ref()),
				consent.account_access(&requester, &target, platform.as_ref()),
				async { release.send(()).unwrap() },
			)
		});
		assert_eq!(
			(first.unwrap(), second.unwrap(), prompts(&platform)),
			(Authorized, Authorized, 1)
		);
	}
}
