//! What the native route reads from and does on the stack's chains, behind a
//! trait so the session flow runs against a fake chain in tests.

use super::{keys::SessionKey, tokens::NativeToken};

/// How a token's deposit becomes CASH on People.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
	/// The native token through the native/CASH pool.
	Pool,
	/// The payment asset itself, sent as it is.
	Direct,
	/// A stable through the PSM's mint, at the fee rate it was quoted, in
	/// parts per million. The call carries the rate as its `max_fee`, so a
	/// change in between fails the call instead of costing more.
	Psm {
		/// The quoted minting fee.
		fee_rate: u32,
	},
	/// A stable the PSM does not take, through the stable/native and the
	/// native/CASH pools.
	StablePool,
}

impl Tier {
	/// The tier as a quote id names it.
	pub fn label(self) -> String {
		match self {
			Tier::Pool => "pool".to_string(),
			Tier::Direct => "direct".to_string(),
			Tier::Psm { fee_rate } => format!("psm-{fee_rate}"),
			Tier::StablePool => "stable-pool".to_string(),
		}
	}

	/// The tier a quote id names.
	pub fn from_label(label: &str) -> Option<Self> {
		match label {
			"pool" => Some(Tier::Pool),
			"direct" => Some(Tier::Direct),
			"stable-pool" => Some(Tier::StablePool),
			_ => label.strip_prefix("psm-")?.parse().ok().map(|fee_rate| Tier::Psm { fee_rate }),
		}
	}
}

/// What it takes to land an amount of CASH on People from a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InPrice {
	/// The tier the deposit converts through.
	pub tier: Tier,
	/// What the user sends, in the token's smallest unit.
	pub send: u128,
	/// The part of `send` that pays chain and PSM fees, or is kept as the
	/// key's minimum balance, in the token's smallest unit.
	pub network_fee: u128,
	/// Whether `send` lands at least the amount whatever the pools do. A
	/// pool tier's price moves until the conversion runs.
	pub exact: bool,
}

/// A chain read or call that failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct ChainError {
	/// What went wrong.
	pub reason: String,
}

impl ChainError {
	/// A failure described by `reason`.
	pub fn new(reason: impl Into<String>) -> Self {
		Self { reason: reason.into() }
	}
}

/// The chain work of the native route.
#[async_trait::async_trait]
pub trait NativeChain: Send + Sync {
	/// The tokens the route can take in now, read live: the pools and the
	/// PSM each one's tier needs exist and hold reserves.
	async fn tokens_in(&self) -> Result<Vec<NativeToken>, ChainError>;

	/// What the user sends in `token` for `cash` CASH to land on People, and
	/// through which tier.
	async fn price_in(&self, token: NativeToken, cash: u128) -> Result<InPrice, ChainError>;

	/// What `account` holds of `token` on Asset Hub at the finalized head.
	async fn balance(&self, token: NativeToken, account: [u8; 32]) -> Result<u128, ChainError>;

	/// The number of transactions `account` has sent on Asset Hub, at the
	/// finalized head.
	async fn nonce(&self, account: [u8; 32]) -> Result<u64, ChainError>;

	/// What `account` holds of CASH on People at the finalized head.
	async fn cash_on_people(&self, account: [u8; 32]) -> Result<u128, ChainError>;

	/// Convert the `deposit` of `token` that `key` holds on Asset Hub through
	/// `tier`, landing at least `min_cash` on the key's People account, signed
	/// at nonce 0 so a repeat after a crash can never run twice. Resolves once
	/// the call is finalized and dispatched.
	async fn convert(
		&self,
		key: &SessionKey,
		token: NativeToken,
		tier: Tier,
		deposit: u128,
		min_cash: u128,
	) -> Result<(), ChainError>;
}

/// Parts in a Permill.
pub const PERMILL: u128 = 1_000_000;

/// `Permill::mul_ceil(amount)`, as the PSM computes its fee.
pub fn permill_mul_ceil(amount: u128, rate: u32) -> u128 {
	(amount * u128::from(rate)).div_ceil(PERMILL)
}

/// What a PSM mint pays out for `gross` at `rate`: the gross less the fee.
pub fn psm_mint_out(gross: u128, rate: u32) -> u128 {
	gross - permill_mul_ceil(gross, rate)
}

/// The least gross a PSM swap at `rate` takes for at least `net` to come out.
/// `None` for a rate that keeps nothing.
pub fn psm_gross_for(net: u128, rate: u32) -> Option<u128> {
	let keep = PERMILL.checked_sub(u128::from(rate)).filter(|keep| *keep > 0)?;
	Some((net * PERMILL).div_ceil(keep))
}

/// Slippage headroom on a pool tier's quote, in basis points.
pub const SLIPPAGE_BPS: u128 = 100;

/// `amount` with [`SLIPPAGE_BPS`] on top, rounded up.
pub fn with_slippage(amount: u128) -> u128 {
	amount + (amount * SLIPPAGE_BPS).div_ceil(10_000)
}

/// `amount` less [`SLIPPAGE_BPS`], as the least a pool tier's exchange may
/// return.
pub fn less_slippage(amount: u128) -> u128 {
	amount - (amount * SLIPPAGE_BPS).div_ceil(10_000)
}

/// The smallest CASH a top-up credits: one cent, as the hosts' engines count.
pub const CREDIT_UNIT: u128 = 10_000;

/// `cash` floored to [`CREDIT_UNIT`].
pub fn creditable(cash: u128) -> u128 {
	cash - cash % CREDIT_UNIT
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_psm_gross_is_the_least_that_nets_the_amount() {
		for (net, rate) in [(10_000_000, 5_000), (1, 5_000), (123_456_789, 0), (7, 999_999)] {
			let gross = psm_gross_for(net, rate).expect("a usable rate");
			assert!(psm_mint_out(gross, rate) >= net, "{net} at {rate}");
			assert!(psm_mint_out(gross - 1, rate) < net, "{net} at {rate}");
		}
		assert_eq!(psm_gross_for(10_000_000, 5_000), Some(10_050_252));
		assert_eq!(psm_gross_for(1, 1_000_000), None);
	}

	#[test]
	fn slippage_and_credits_round_the_safe_way() {
		assert_eq!(with_slippage(10_000), 10_100);
		assert_eq!(with_slippage(1), 2);
		assert_eq!(less_slippage(10_000), 9_900);
		assert_eq!(creditable(10_049_999), 10_040_000);
		assert_eq!(creditable(9_999), 0);
	}

	#[test]
	fn tiers_round_trip_through_their_labels() {
		for tier in [Tier::Pool, Tier::Direct, Tier::StablePool, Tier::Psm { fee_rate: 5_000 }] {
			assert_eq!(Tier::from_label(&tier.label()), Some(tier));
		}
		assert_eq!(Tier::from_label("psm-x"), None);
		assert_eq!(Tier::from_label("teleport"), None);
	}
}
