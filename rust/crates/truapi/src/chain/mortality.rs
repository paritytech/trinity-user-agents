//! [`Mortality`]: the era a mortal extrinsic was signed with, fully referenced.

use super::{EncodedExtrinsic, HashAndNumber};

/// The mortal era an extrinsic was signed with: the block it was born at and
/// how many blocks it stays valid for.
///
/// The birth block's hash is what `CheckMortality` signs over without putting
/// it in the bytes, so whoever built the extrinsic has to supply it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mortality {
	birth: HashAndNumber,
	period: u64,
}

/// Why a birth block and period do not describe a mortal era.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MortalityError {
	/// Eras have a power-of-two period between 4 and 65536.
	#[error("{0} is not a mortal era period, expected a power of two from 4 to 65536")]
	InvalidPeriod(u64),
	/// An era with this period cannot be born at this block: periods above
	/// 4096 quantize the birth block, and the extrinsic would name another one.
	#[error(
		"no era with period {period} is born at block {birth}, expected a birth whose phase \
         is a multiple of period / 4096"
	)]
	UnalignedBirth {
		/// Birth block number.
		birth: u64,
		/// Era period.
		period: u64,
	},
}

impl Mortality {
	/// The era born at `birth` with `period`, exactly as `CheckMortality`
	/// encodes it.
	pub fn new(birth: HashAndNumber, period: u64) -> Result<Self, MortalityError> {
		if !period.is_power_of_two() || !(4..=65_536).contains(&period) {
			return Err(MortalityError::InvalidPeriod(period));
		}
		let quantize_factor = (period >> 12).max(1);
		if !(birth.number % period).is_multiple_of(quantize_factor) {
			return Err(MortalityError::UnalignedBirth { birth: birth.number, period });
		}
		Ok(Self { birth, period })
	}

	/// The block the era was born at.
	pub fn birth(&self) -> HashAndNumber {
		self.birth
	}

	/// How many blocks the era lasts.
	pub fn period(&self) -> u64 {
		self.period
	}

	/// The first block that can no longer include the extrinsic.
	pub fn death(&self) -> u64 {
		self.birth.number + self.period
	}
}

/// An encoded extrinsic together with the era it was signed with. There is no
/// immortal counterpart: a durable submission must be able to expire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MortalExtrinsic {
	/// The encoded extrinsic.
	pub extrinsic: EncodedExtrinsic,
	/// The era it was signed with.
	pub mortality: Mortality,
}

#[cfg(test)]
mod tests;
