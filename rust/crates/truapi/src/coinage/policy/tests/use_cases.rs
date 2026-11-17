//! Who may spend held-back value, as payments ask it.

use super::*;

/// The most private policy never lets a spend the user starts take value
/// still gaining privacy, so it offers no confirmation for it; balanced
/// offers one, and a product's spend asks for one under both.
#[test]
fn the_privacy_presets_decide_whether_held_back_value_is_offered() {
	let offered = |policy: PrivacyPolicy| (policy.held_back_for(Initiator::User), policy.held_back_for(Initiator::Product));
	assert_eq!(offered(PrivacyPolicy::most_private()), (HeldBackSpend::Never, HeldBackSpend::AfterConfirmation));
	assert_eq!(offered(PrivacyPolicy::balanced()), (HeldBackSpend::AfterConfirmation, HeldBackSpend::AfterConfirmation));
}
