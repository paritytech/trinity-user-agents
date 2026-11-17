//! A1 · Denominations: a coin of exponent `e` is worth `2^e` cents, and an
//! amount is a whole number of cents.

use parity_scale_codec::{Decode, Encode};

use super::ChainParams;

/// A coin's exponent: the coin is worth `2^e` cents.
///
/// Every exponent is below 64, so its value fits a `u64` amount. An exponent
/// read from the chain becomes one only through [`Exponent::from_chain`],
/// which also checks it against the legal range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct Exponent(u8);

uniffi::custom_type!(Exponent, u8, {
	lower: |exponent| exponent.0,
	try_lift: |raw| Exponent::new(raw).ok_or_else(|| uniffi::deps::anyhow::anyhow!("{raw} is not a legal exponent")),
});

impl Exponent {
	/// The exponent `raw`, or none if `2^raw` cents does not fit a `u64`.
	pub const fn new(raw: u8) -> Option<Self> {
		if raw < 64 { Some(Self(raw)) } else { None }
	}

	/// The exponent as a number.
	pub const fn get(self) -> u8 {
		self.0
	}

	/// `2^e` cents.
	pub const fn value(self) -> u64 {
		1 << self.0
	}

	/// The exponent the chain stores in a coin, or none for a negative one or
	/// one above the largest legal exponent (A1 invariant 1). A negative
	/// exponent is a coin smaller than a cent, which this layer never holds,
	/// and is refused rather than rounded down.
	pub fn from_chain(chain_value: i8, params: &ChainParams) -> Option<Self> {
		let raw = u8::try_from(chain_value).ok()?;
		let exponent = Self::new(raw)?;
		(exponent <= params.largest_exponent()).then_some(exponent)
	}
}

/// The coins that make up `amount` with the fewest coins, largest first.
///
/// With `m` the largest legal exponent, that is `amount / 2^m` coins of
/// exponent `m`, followed by one coin for each bit set in `amount mod 2^m`.
/// The result is one multiset only, so every implementation returns the same
/// coins (P4).
pub fn denominations(amount: u64, params: &ChainParams) -> Vec<Exponent> {
	let largest = params.largest_exponent();
	let whole = amount >> largest.get();
	let remainder = amount & (largest.value() - 1);
	let largest_coins = (0..whole).map(|_| largest);
	let remainder_coins = (0..largest.get())
		.rev()
		.filter(|bit| remainder & (1 << bit) != 0)
		.filter_map(Exponent::new);
	largest_coins.chain(remainder_coins).collect()
}

#[cfg(test)]
mod tests;
