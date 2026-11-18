//! A4 · Privacy policy: how much availability the user gives up for privacy,
//! carried as data.
//!
//! Components read only the values, never a preset's name, and each applies
//! its own logic to them (invariant 1). The age cutoff bounds the policy
//! through [`effective`]; the token reserve bounds it in D1.

use core::time::Duration;

use super::ChainParams;

/// The three privacy settings the shipped wallets offer the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PrivacyPreset {
	/// [`PrivacyPolicy::fastest`].
	Fastest,
	/// [`PrivacyPolicy::balanced`].
	Balanced,
	/// [`PrivacyPolicy::most_private`].
	MostPrivate,
}

impl PrivacyPreset {
	/// The policy this setting stands for.
	pub fn policy(self) -> PrivacyPolicy {
		match self {
			Self::Fastest => PrivacyPolicy::fastest(),
			Self::Balanced => PrivacyPolicy::balanced(),
			Self::MostPrivate => PrivacyPolicy::most_private(),
		}
	}
}

/// The user's privacy choice, as the components read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivacyPolicy {
	/// When an entry counts as ready: when any of these rules holds.
	pub ready_when: Vec<ReadyRule>,
	/// The coin age from which the recycling sweep takes a coin.
	pub recycle_at: RecycleAge,
	/// How much of a purse's total, in percent, may be unavailable when the
	/// recycling sweep admits another coin.
	pub max_unavailable_share: u8,
	/// Whether a spend the user starts may take held-back value.
	pub held_back_for_user: HeldBackSpend,
	/// Whether a spend layer 2 makes for a product may take held-back value.
	pub held_back_for_products: HeldBackSpend,
	/// Whether moving value between purses goes through the recycler.
	pub move_through_recycler: bool,
}

/// One readiness rule: the ring's root includes at least `members` members,
/// and `dwell` has passed since the entry's own key was first observed
/// included in that root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadyRule {
	/// How many members the ring's root must include.
	pub members: MemberThreshold,
	/// How long the entry's key must have been included.
	pub dwell: Duration,
}

/// A number of ring members.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberThreshold {
	/// That many members.
	Count(u32),
	/// That percentage of the ring size, rounded up.
	PercentOfRingSize(u8),
}

impl MemberThreshold {
	/// The number of members for a ring of `ring_size`.
	pub fn members(self, ring_size: u32) -> u32 {
		match self {
			Self::Count(count) => count,
			Self::PercentOfRingSize(percent) => {
				let members = (u64::from(ring_size) * u64::from(percent)).div_ceil(100);
				u32::try_from(members).unwrap_or(u32::MAX)
			},
		}
	}
}

/// The coin age from which the recycling sweep takes a coin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecycleAge {
	/// This age.
	Age(u32),
	/// The age cutoff divided by this, rounded down, and at least 1.
	CutoffDividedBy(u32),
}

/// Whether a spend may take held-back value when the value that is not held
/// back does not cover it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldBackSpend {
	/// It does not.
	Never,
	/// Only once the user has confirmed it.
	AfterConfirmation,
	/// Without asking.
	Always,
}

/// Who asked for a spend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
pub enum Initiator {
	/// The user.
	User,
	/// Layer 2, for a product.
	Product,
}

impl PrivacyPolicy {
	/// The setting for spends `initiator` starts.
	pub fn held_back_for(&self, initiator: Initiator) -> HeldBackSpend {
		match initiator {
			Initiator::User => self.held_back_for_user,
			Initiator::Product => self.held_back_for_products,
		}
	}

	/// The shipped wallet's *Fastest* setting, the initial policy of a host
	/// that follows it.
	pub fn fastest() -> Self {
		Self {
			ready_when: vec![rule(MemberThreshold::PercentOfRingSize(0), 0)],
			recycle_at: RecycleAge::CutoffDividedBy(1),
			max_unavailable_share: 0,
			held_back_for_user: HeldBackSpend::AfterConfirmation,
			held_back_for_products: HeldBackSpend::Always,
			move_through_recycler: false,
		}
	}

	/// The shipped wallet's *Balanced* setting.
	pub fn balanced() -> Self {
		Self {
			ready_when: vec![
				rule(MemberThreshold::PercentOfRingSize(20), 0),
				rule(MemberThreshold::Count(32), 10 * 60),
			],
			recycle_at: RecycleAge::CutoffDividedBy(3),
			max_unavailable_share: 20,
			held_back_for_user: HeldBackSpend::AfterConfirmation,
			held_back_for_products: HeldBackSpend::AfterConfirmation,
			move_through_recycler: true,
		}
	}

	/// The shipped wallet's *Most private* setting.
	pub fn most_private() -> Self {
		Self {
			ready_when: vec![
				rule(MemberThreshold::PercentOfRingSize(90), 0),
				rule(MemberThreshold::Count(32), 10 * 60),
			],
			recycle_at: RecycleAge::Age(1),
			max_unavailable_share: 100,
			held_back_for_user: HeldBackSpend::Never,
			held_back_for_products: HeldBackSpend::AfterConfirmation,
			move_through_recycler: true,
		}
	}
}

fn rule(members: MemberThreshold, dwell_seconds: u64) -> ReadyRule {
	ReadyRule { members, dwell: Duration::from_secs(dwell_seconds) }
}

/// Why a policy is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum PolicyError {
	/// It has no readiness rule, so no entry could ever be ready.
	#[display("the policy has no readiness rule")]
	NoReadyRule,
	/// The share or a ring percentage is above 100.
	#[display("a percentage in the policy is above 100")]
	PercentAbove100,
	/// The recycle age divides the cutoff by zero.
	#[display("the policy divides the age cutoff by zero")]
	ZeroDivisor,
}

/// Refuses a policy no component could apply.
pub fn check(policy: &PrivacyPolicy) -> Result<(), PolicyError> {
	if policy.ready_when.is_empty() {
		return Err(PolicyError::NoReadyRule);
	}
	let ring_percent_above = policy
		.ready_when
		.iter()
		.any(|rule| matches!(rule.members, MemberThreshold::PercentOfRingSize(percent) if percent > 100));
	if policy.max_unavailable_share > 100 || ring_percent_above {
		return Err(PolicyError::PercentAbove100);
	}
	if policy.recycle_at == RecycleAge::CutoffDividedBy(0) {
		return Err(PolicyError::ZeroDivisor);
	}
	Ok(())
}

/// `policy` with its recycle age resolved to an age no later than the age
/// cutoff, so no policy postpones recycling past it. Every component reads
/// the policy through this.
pub fn effective(policy: &PrivacyPolicy, params: &ChainParams) -> PrivacyPolicy {
	let cutoff = u32::from(params.age_cutoff());
	let age = match policy.recycle_at {
		RecycleAge::Age(age) => age,
		RecycleAge::CutoffDividedBy(divisor) => (cutoff / divisor.max(1)).max(1),
	};
	PrivacyPolicy { recycle_at: RecycleAge::Age(age.min(cutoff)), ..policy.clone() }
}

#[cfg(test)]
mod tests;
