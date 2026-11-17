use super::*;
use crate::coinage::{
	params::validate,
	testing::{chain_config, chain_params, instance_record, people_metadata},
};

fn params_with_cutoff(maximum_age: u16) -> ChainParams {
	validate(&people_metadata(), Some(&instance_record()), Some(maximum_age), chain_config()).unwrap()
}

#[test]
fn the_presets_pass_the_check() {
	for preset in [PrivacyPreset::Fastest, PrivacyPreset::Balanced, PrivacyPreset::MostPrivate] {
		assert_eq!(check(&preset.policy()), Ok(()));
	}
}

#[test]
fn the_check_refuses_a_policy_no_component_could_apply() {
	let none = PrivacyPolicy { ready_when: Vec::new(), ..PrivacyPolicy::balanced() };
	assert_eq!(check(&none), Err(PolicyError::NoReadyRule));
	let share = PrivacyPolicy { max_unavailable_share: 101, ..PrivacyPolicy::balanced() };
	assert_eq!(check(&share), Err(PolicyError::PercentAbove100));
	let ring = PrivacyPolicy {
		ready_when: vec![ReadyRule { members: MemberThreshold::PercentOfRingSize(101), dwell: Duration::ZERO }],
		..PrivacyPolicy::balanced()
	};
	assert_eq!(check(&ring), Err(PolicyError::PercentAbove100));
	let divisor = PrivacyPolicy { recycle_at: RecycleAge::CutoffDividedBy(0), ..PrivacyPolicy::balanced() };
	assert_eq!(check(&divisor), Err(PolicyError::ZeroDivisor));
}

#[test]
fn the_effective_recycle_age_never_passes_the_age_cutoff() {
	let params = chain_params();
	assert_eq!(params.age_cutoff(), 14);
	let at = |policy: PrivacyPolicy| effective(&policy, &params).recycle_at;
	assert_eq!(at(PrivacyPolicy::fastest()), RecycleAge::Age(14));
	assert_eq!(at(PrivacyPolicy::balanced()), RecycleAge::Age(4));
	assert_eq!(at(PrivacyPolicy::most_private()), RecycleAge::Age(1));
	let late = PrivacyPolicy { recycle_at: RecycleAge::Age(40), ..PrivacyPolicy::balanced() };
	assert_eq!(at(late), RecycleAge::Age(14));
}

#[test]
fn a_lower_cutoff_takes_effect_without_a_stored_change() {
	let params = params_with_cutoff(4);
	assert_eq!(params.age_cutoff(), 2);
	assert_eq!(effective(&PrivacyPolicy::fastest(), &params).recycle_at, RecycleAge::Age(2));
	assert_eq!(effective(&PrivacyPolicy::balanced(), &params).recycle_at, RecycleAge::Age(1));
}

#[test]
fn a_ring_percentage_is_rounded_up() {
	assert_eq!(MemberThreshold::PercentOfRingSize(20).members(767), 154);
	assert_eq!(MemberThreshold::PercentOfRingSize(90).members(767), 691);
	assert_eq!(MemberThreshold::PercentOfRingSize(0).members(767), 0);
	assert_eq!(MemberThreshold::Count(32).members(767), 32);
}

#[test]
fn each_initiator_reads_its_own_setting() {
	let policy = PrivacyPolicy::fastest();
	assert_eq!(policy.held_back_for(Initiator::User), HeldBackSpend::AfterConfirmation);
	assert_eq!(policy.held_back_for(Initiator::Product), HeldBackSpend::Always);
}

mod use_cases;
