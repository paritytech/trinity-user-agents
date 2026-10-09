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
            return Err(MortalityError::UnalignedBirth {
                birth: birth.number,
                period,
            });
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
mod tests {
    use subxt::utils::H256;

    use super::*;

    fn block(number: u64) -> HashAndNumber {
        HashAndNumber {
            hash: H256::from_low_u64_be(number),
            number,
        }
    }

    #[test]
    fn an_era_is_born_at_its_block_and_dies_a_period_later() {
        let mortality = Mortality::new(block(100), 64).unwrap();

        assert_eq!(mortality.birth(), block(100));
        assert_eq!(mortality.period(), 64);
        assert_eq!(mortality.death(), 164);
    }

    /// `Era::mortal` rounds a period up to a power of two, so a period that
    /// is not one would record a window the extrinsic does not have.
    #[test]
    fn a_period_that_is_not_an_era_period_is_rejected() {
        for period in [0, 2, 3, 100, 131_072] {
            assert_eq!(
                Mortality::new(block(100), period),
                Err(MortalityError::InvalidPeriod(period))
            );
        }
    }

    /// Above 4096 the era phase is stored in steps of `period / 4096`, so
    /// only every such step can be a birth block.
    #[test]
    fn a_long_era_is_born_only_on_its_quantized_phase() {
        assert_eq!(
            Mortality::new(block(8193), 8192),
            Err(MortalityError::UnalignedBirth {
                birth: 8193,
                period: 8192
            })
        );
        assert!(Mortality::new(block(8194), 8192).is_ok());
        assert!(Mortality::new(block(4097), 4096).is_ok());
    }
}
