use subxt::utils::H256;

use super::*;

fn block(number: u64) -> HashAndNumber {
	HashAndNumber { hash: H256::from_low_u64_be(number), number }
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
		assert_eq!(Mortality::new(block(100), period), Err(MortalityError::InvalidPeriod(period)));
	}
}

/// Above 4096 the era phase is stored in steps of `period / 4096`, so
/// only every such step can be a birth block.
#[test]
fn a_long_era_is_born_only_on_its_quantized_phase() {
	assert_eq!(
		Mortality::new(block(8193), 8192),
		Err(MortalityError::UnalignedBirth { birth: 8193, period: 8192 })
	);
	assert!(Mortality::new(block(8194), 8192).is_ok());
	assert!(Mortality::new(block(4097), 4096).is_ok());
}
