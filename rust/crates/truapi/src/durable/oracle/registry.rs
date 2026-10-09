//! [`DurableRegistry`]: the oracle of every domain the engine serves.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use subxt::utils::H256;

use super::CompletionOracle;
use crate::durable::model::DomainId;

/// The oracle of every domain the engine serves. A domain without one cannot
/// register transactions.
#[derive(Default)]
pub struct DurableRegistry {
    oracles: HashMap<DomainId, Arc<dyn CompletionOracle>>,
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
        assert!(
            !self.oracles.contains_key(&domain),
            "durable domain {domain:?} registered twice"
        );
        self.oracles.insert(domain, oracle);
        self
    }

    /// The oracle of `domain`.
    pub fn oracle(&self, domain: &DomainId) -> Option<&Arc<dyn CompletionOracle>> {
        self.oracles.get(domain)
    }

    /// Genesis hashes of every chain a registered domain lives on.
    pub fn chains(&self) -> BTreeSet<H256> {
        self.oracles.values().map(|oracle| oracle.chain()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::durable::oracle::Unobservable;

    #[test]
    #[should_panic(expected = "registered twice")]
    fn a_domain_registered_twice_is_a_wiring_bug() {
        let oracle: Arc<dyn CompletionOracle> = Arc::new(Unobservable(H256::zero()));
        let _ = DurableRegistry::new()
            .with_domain(DomainId::new("test"), oracle.clone())
            .with_domain(DomainId::new("test"), oracle);
    }
}
