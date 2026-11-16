use super::*;
use crate::coinage::testing::chain_params;

fn raw(exponents: &[Exponent]) -> Vec<u8> {
	exponents.iter().map(|exponent| exponent.get()).collect()
}

#[test]
fn denominations_sum_to_the_amount_with_the_fewest_coins() {
	let params = chain_params();
	for amount in (0..70_000).chain([u64::from(u32::MAX), 1 << 40]) {
		let coins = denominations(amount, &params);
		assert_eq!(coins.iter().map(|coin| coin.value()).sum::<u64>(), amount);
		let whole = amount >> 14;
		let fewest = whole + u64::from((amount & ((1 << 14) - 1)).count_ones());
		assert_eq!(coins.len() as u64, fewest, "amount {amount}");
		assert!(coins.windows(2).all(|pair| pair[0] >= pair[1]), "largest first for {amount}");
	}
}

#[test]
fn denominations_at_the_boundaries_of_the_range() {
	let params = chain_params();
	assert_eq!(raw(&denominations(0, &params)), Vec::<u8>::new());
	assert_eq!(raw(&denominations(1, &params)), [0]);
	assert_eq!(raw(&denominations(16_383, &params)), (0..14).rev().collect::<Vec<_>>());
	assert_eq!(raw(&denominations(16_384, &params)), [14]);
	assert_eq!(raw(&denominations(16_385, &params)), [14, 0]);
	assert_eq!(raw(&denominations(3 * 16_384 + 2, &params)), [14, 14, 14, 1]);
}

#[test]
fn a_chain_exponent_outside_the_legal_range_is_refused() {
	let params = chain_params();
	for negative in i8::MIN..0 {
		assert_eq!(Exponent::from_chain(negative, &params), None);
	}
	for above in 15..=i8::MAX {
		assert_eq!(Exponent::from_chain(above, &params), None);
	}
	for legal in 0..=14 {
		assert_eq!(Exponent::from_chain(legal, &params).map(Exponent::get), Some(legal as u8));
	}
}

#[test]
fn an_exponent_is_worth_two_to_its_power() {
	assert_eq!(Exponent::new(0).map(Exponent::value), Some(1));
	assert_eq!(Exponent::new(14).map(Exponent::value), Some(16_384));
	assert_eq!(Exponent::new(63).map(Exponent::value), Some(1 << 63));
	assert_eq!(Exponent::new(64), None);
}
