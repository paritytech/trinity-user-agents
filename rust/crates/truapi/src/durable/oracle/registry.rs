//! [`DurableRegistry`]: every domain the engine serves, with the chain it
//! lives on, the oracle that decides it and the policy that builds it.

use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use subxt::utils::H256;

use super::CompletionOracle;
use crate::durable::{model::DomainId, policy::SubmissionPolicy};

/// Every domain the engine serves. A domain missing from it cannot register
/// transactions.
#[derive(Default)]
pub struct DurableRegistry {
	domains: HashMap<DomainId, RegisteredDomain>,
}

/// What serves one domain.
struct RegisteredDomain {
	chain: H256,
	oracle: Arc<dyn CompletionOracle>,
	policy: Arc<dyn SubmissionPolicy>,
}

impl DurableRegistry {
	/// A registry with no domains.
	pub fn new() -> Self {
		Self::default()
	}

	/// Adds `domain`, living on the chain with genesis hash `chain`, decided
	/// by `oracle` and built by `policy`.
	///
	/// # Panics
	///
	/// When `domain` is already registered: two registrations of one domain
	/// is a wiring bug.
	pub fn with_domain(
		mut self,
		domain: DomainId,
		chain: H256,
		oracle: Arc<dyn CompletionOracle>,
		policy: Arc<dyn SubmissionPolicy>,
	) -> Self {
		assert!(!self.domains.contains_key(&domain), "durable domain {domain:?} registered twice");
		self.domains.insert(domain, RegisteredDomain { chain, oracle, policy });
		self
	}

	/// The policy that builds the transactions of `domain`.
	pub fn submission_policy(&self, domain: &DomainId) -> Option<&Arc<dyn SubmissionPolicy>> {
		self.domains.get(domain).map(|registered| &registered.policy)
	}

	/// The oracle of `domain`.
	pub fn oracle(&self, domain: &DomainId) -> Option<&Arc<dyn CompletionOracle>> {
		self.domains.get(domain).map(|registered| &registered.oracle)
	}

	/// Genesis hash of the chain `domain` lives on.
	pub fn chain(&self, domain: &DomainId) -> Option<H256> {
		self.domains.get(domain).map(|registered| registered.chain)
	}

	/// Whether any of `domains` is registered. Transactions of anything else
	/// are left as they are: the domain may be back on a later start.
	pub fn serves_any(&self, domains: &[DomainId]) -> bool {
		domains.iter().any(|domain| self.domains.contains_key(domain))
	}

	/// Genesis hashes of every chain a registered domain lives on.
	pub fn chains(&self) -> BTreeSet<H256> {
		self.domains.values().map(|registered| registered.chain).collect()
	}
}

#[cfg(test)]
mod tests;
