use super::*;
use crate::durable::{oracle::Unobservable, testing::ScriptedPolicy};

#[test]
#[should_panic(expected = "registered twice")]
fn a_domain_registered_twice_is_a_wiring_bug() {
	let oracle: Arc<dyn CompletionOracle> = Arc::new(Unobservable(H256::zero()));
	let _ = DurableRegistry::new()
		.with_domain(DomainId::new("test"), oracle.clone())
		.with_domain(DomainId::new("test"), oracle);
}

#[test]
#[should_panic(expected = "registered twice")]
fn a_policy_registered_twice_is_a_wiring_bug() {
	let _ = DurableRegistry::new()
		.with_policy(Arc::new(ScriptedPolicy::never_answering()))
		.with_policy(Arc::new(ScriptedPolicy::giving_up()));
}
