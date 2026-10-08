//! [`DurableRegistry`]: the oracle of every domain the engine serves and
//! every submission policy that builds its transactions.

use std::collections::HashMap;
use std::sync::Arc;

use subxt::utils::H256;

use super::CompletionOracle;
use crate::durable::model::{DomainId, PolicyId};
use crate::durable::policy::SubmissionPolicy;

/// The oracle of every domain the engine serves, and every submission policy.
/// A domain without an oracle cannot register transactions.
#[derive(Default)]
pub struct DurableRegistry {
    oracles: HashMap<DomainId, Arc<dyn CompletionOracle>>,
    policies: HashMap<PolicyId, Arc<dyn SubmissionPolicy>>,
}

impl DurableRegistry {
    /// A registry with no domains.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `domain`, decided by `oracle`.
    ///
    /// # Panics
    ///
    /// When `domain` is already registered: two oracles for one domain is a
    /// wiring bug.
    pub fn with_domain(mut self, domain: DomainId, oracle: Arc<dyn CompletionOracle>) -> Self {
        let duplicate = domain.clone();
        assert!(
            self.oracles.insert(domain, oracle).is_none(),
            "durable domain {} registered twice",
            duplicate.as_str()
        );
        self
    }

    /// Adds `policy` under its own id.
    ///
    /// # Panics
    ///
    /// When a policy with that id is already registered: rows naming it
    /// could not tell the two apart.
    pub fn with_policy(mut self, policy: Arc<dyn SubmissionPolicy>) -> Self {
        let id = policy.id();
        let name = id.as_str().to_owned();
        assert!(
            self.policies.insert(id, policy).is_none(),
            "submission policy {name} registered twice"
        );
        self
    }

    /// The policy registered as `id`.
    pub fn submission_policy(&self, id: &PolicyId) -> Option<&Arc<dyn SubmissionPolicy>> {
        self.policies.get(id)
    }

    /// The oracle of `domain`.
    pub fn oracle(&self, domain: &DomainId) -> Option<&Arc<dyn CompletionOracle>> {
        self.oracles.get(domain)
    }

    /// Genesis hashes of every chain a registered domain lives on.
    pub fn chains(&self) -> Vec<H256> {
        let mut chains: Vec<H256> = self.oracles.values().map(|oracle| oracle.chain()).collect();
        chains.sort();
        chains.dedup();
        chains
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::durable::oracle::Unobservable;
    use crate::durable::testing::{REBUILD_POLICY, RetryAnswer, ScriptedPolicy};

    #[test]
    #[should_panic(expected = "registered twice")]
    fn a_domain_registered_twice_is_a_wiring_bug() {
        let oracle: Arc<dyn CompletionOracle> = Arc::new(Unobservable(H256::zero()));
        let _ = DurableRegistry::new()
            .with_domain(DomainId::from_static("test"), oracle.clone())
            .with_domain(DomainId::from_static("test"), oracle);
    }

    #[test]
    #[should_panic(expected = "registered twice")]
    fn a_policy_registered_twice_is_a_wiring_bug() {
        let _ = DurableRegistry::new()
            .with_policy(ScriptedPolicy::never_building(REBUILD_POLICY, RetryAnswer::Final))
            .with_policy(ScriptedPolicy::never_building(REBUILD_POLICY, RetryAnswer::Retry));
    }
}
