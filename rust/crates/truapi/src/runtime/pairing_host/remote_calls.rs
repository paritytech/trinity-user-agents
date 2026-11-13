//! Grants a pairing host keeps from its paired signing host's answers.

use super::{
	super::{
		allowances::AllowanceResource,
		authority::{
			AccountCaller, AccountGrant, AccountGrantOutcome, AccountHolder, AccountInvocation,
			Allowance, AuthorityError, AuthoritySession, BulletinAllowanceKey,
			StatementStoreAllowanceKey,
		},
	},
	PairingHost, product_invocation,
};
use crate::{
	host_internal::sso_messages::OnExistingAllowancePolicy,
	host_logic::session::SessionInfo,
	platform::{ProductContext, ProductId},
};

use futures::StreamExt;
use truapi::{CallContext, latest};

impl PairingHost {
	/// Resolve a product's hard-subtree public key, asking the Account Holder
	/// only when neither the memory cache nor storage already holds it.
	pub async fn remote_product_subtree_public_key(
		&self,
		cx: &CallContext,
		authority_session: &AuthoritySession,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<[u8; 32], AuthorityError> {
		if session.sso.is_none() {
			return Err(AuthorityError::Disconnected);
		}
		let grant_revision = self.grants.lifecycle().revision();
		if let Some(public_key) = self.grants.known_product_subtree(session, product_id).await {
			return Ok(public_key);
		}
		let product = ProductContext::new(product_id.clone());
		let public_key = self
			.holder
			.product_subtree_public_key(
				product_invocation(cx, authority_session, Some(&product)),
				product_id,
			)
			.await?;
		if !self
			.grants
			.persist_product_subtree_if_current(session, grant_revision, product_id, public_key)
			.await
		{
			return Err(AuthorityError::Disconnected);
		}
		Ok(public_key)
	}

	/// Ask the paired signing host to allocate product resources and keep the grants.
	///
	/// `grant_revision` is read before the allocation was confirmed, so a
	/// product reset during the review discards the result.
	pub async fn remote_allocate_resources(
		&self,
		cx: &CallContext,
		authority_session: &AuthoritySession,
		session: &SessionInfo,
		grant_revision: u64,
		product: &ProductContext,
		request: latest::HostRequestResourceAllocationRequest,
	) -> Result<latest::HostRequestResourceAllocationResponse, AuthorityError> {
		self.grants.lifecycle().require_revision(grant_revision)?;
		let invocation = AccountInvocation {
			call: cx,
			session: authority_session,
			caller: AccountCaller::Local { product },
		};
		let mut grants = self
			.holder
			.allocate_grants(invocation, request, OnExistingAllowancePolicy::Increase)
			.await?;
		let mut outcomes = Vec::new();
		while let Some(outcome) = grants.next().await {
			outcomes.push(match outcome? {
				AccountGrantOutcome::Allocated(grant) => {
					self.retain_grant(
						cx,
						authority_session,
						session,
						grant_revision,
						&product.product_id,
						grant,
					)
					.await?;
					latest::AllocationOutcome::Allocated
				},
				AccountGrantOutcome::Rejected => latest::AllocationOutcome::Rejected,
				AccountGrantOutcome::NotAvailable { .. } => latest::AllocationOutcome::NotAvailable,
			});
		}
		Ok(latest::HostRequestResourceAllocationResponse { outcomes })
	}

	/// Statement-store allowance key for the product, served from the cache
	/// or allocated by the paired signing host.
	pub async fn remote_statement_store_allowance_key(
		&self,
		cx: &CallContext,
		authority_session: &AuthoritySession,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<StatementStoreAllowanceKey, AuthorityError> {
		let grant_revision = self.grants.lifecycle().revision();
		if let Some(Allowance::StatementStore { key, .. }) = self
			.grants
			.cached_allowance(
				session,
				grant_revision,
				product_id,
				AllowanceResource::StatementStore,
			)
			.await?
		{
			return Ok(key);
		}
		let product = ProductContext::new(product_id.clone());
		let grant = self
			.holder
			.ensure_allowance(
				product_invocation(cx, authority_session, Some(&product)),
				AllowanceResource::StatementStore,
				OnExistingAllowancePolicy::Ignore,
			)
			.await?;
		match &grant {
			AccountGrant::Allowance(allowance @ Allowance::StatementStore { key, .. }) => {
				self.grants
					.retain_allowance(session, grant_revision, product_id, allowance)
					.await?;
				Ok(key.clone())
			},
			_ => Err(AuthorityError::Unknown {
				reason: "Unexpected statement-store allowance response resource".to_string(),
			}),
		}
	}

	/// Bulletin allowance key for the product, served from the cache or
	/// allocated by the paired signing host.
	pub async fn remote_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		authority_session: &AuthoritySession,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let grant_revision = self.grants.lifecycle().revision();
		if let Some(Allowance::Bulletin(key)) = self
			.grants
			.cached_allowance(session, grant_revision, product_id, AllowanceResource::Bulletin)
			.await?
		{
			return Ok(key);
		}
		self.allocate_bulletin_allowance_key(
			cx,
			authority_session,
			session,
			grant_revision,
			product_id,
			OnExistingAllowancePolicy::Ignore,
		)
		.await
	}

	/// Evict the cached Bulletin allowance key and allocate a fresh one with
	/// an increased allowance, so a stale or exhausted slot is never reused.
	pub async fn remote_refresh_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		authority_session: &AuthoritySession,
		session: &SessionInfo,
		product_id: &ProductId,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let grant_revision = self.grants.lifecycle().revision();
		self.grants
			.evict_bulletin_allowance(session, grant_revision, product_id)
			.await?;
		self.allocate_bulletin_allowance_key(
			cx,
			authority_session,
			session,
			grant_revision,
			product_id,
			OnExistingAllowancePolicy::Increase,
		)
		.await
	}

	async fn allocate_bulletin_allowance_key(
		&self,
		cx: &CallContext,
		authority_session: &AuthoritySession,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		on_existing: OnExistingAllowancePolicy,
	) -> Result<BulletinAllowanceKey, AuthorityError> {
		let product = ProductContext::new(product_id.clone());
		let grant = self
			.holder
			.ensure_allowance(
				product_invocation(cx, authority_session, Some(&product)),
				AllowanceResource::Bulletin,
				on_existing,
			)
			.await?;
		match &grant {
			AccountGrant::Allowance(allowance @ Allowance::Bulletin(key)) => {
				self.grants
					.retain_allowance(session, grant_revision, product_id, allowance)
					.await?;
				Ok(key.clone())
			},
			_ => Err(AuthorityError::Unknown {
				reason: "Unexpected bulletin allowance response resource".to_string(),
			}),
		}
	}

	/// Keep one grant the paired signing host issued for this host's product.
	async fn retain_grant(
		&self,
		cx: &CallContext,
		authority_session: &AuthoritySession,
		session: &SessionInfo,
		grant_revision: u64,
		product_id: &ProductId,
		grant: AccountGrant,
	) -> Result<(), AuthorityError> {
		match grant {
			AccountGrant::Allowance(allowance) => {
				self.grants
					.retain_allowance(session, grant_revision, product_id, &allowance)
					.await
			},
			AccountGrant::AutoSigning(key) => {
				let expected = self
					.remote_product_subtree_public_key(cx, authority_session, session, product_id)
					.await?;
				self.grants
					.remember_auto_signing_key(session, grant_revision, product_id, expected, key)
					.await
			},
			AccountGrant::SmartContract => Ok(()),
		}
	}
}
