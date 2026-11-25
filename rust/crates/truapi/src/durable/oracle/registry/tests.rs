use super::*;
use crate::durable::{oracle::Unobservable, testing::ScriptedPolicy};

#[test]
#[should_panic(expected = "registered twice")]
fn a_domain_registered_twice_is_a_wiring_bug() {
	let oracle: Arc<dyn CompletionOracle> = Arc::new(Unobservable);
	let policy: Arc<dyn SubmissionPolicy> = Arc::new(ScriptedPolicy::never_answering());
	let _ = DurableRegistry::new()
		.with_domain(DomainId::new("test"), H256::zero(), oracle.clone(), policy.clone())
		.with_domain(DomainId::new("test"), H256::zero(), oracle, policy);
}
