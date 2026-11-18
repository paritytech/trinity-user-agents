use super::*;
use crate::coinage::testing::chain_params;

fn exponent(raw: u8) -> Exponent {
	Exponent::new(raw).unwrap()
}

/// The coins of one holding, one count per exponent from 0.
fn holding(counts: &[u32]) -> Vec<(Exponent, u32)> {
	counts.iter().enumerate().map(|(raw, count)| (exponent(raw as u8), *count)).collect()
}

fn raw_plan(plan: &[Split]) -> Vec<(u8, Vec<u8>)> {
	plan.iter()
		.map(|split| (split.consume.get(), split.produce.iter().map(|e| e.get()).collect()))
		.collect()
}

/// Whether some subset of `coins` (exponents) sums exactly to `amount`.
fn reachable(amount: u64, coins: &[u8]) -> bool {
	let mut sums: Vec<bool> = vec![false; amount as usize + 1];
	sums[0] = true;
	for coin in coins {
		let value = 1usize << coin;
		for sum in (value..sums.len()).rev() {
			sums[sum] |= sums[sum - value];
		}
	}
	sums[amount as usize]
}

/// The coins of `counts` after applying `plan`, as exponents.
fn applied(counts: &[u32], plan: &[Split]) -> Vec<u8> {
	let mut held = counts.to_vec();
	for split in plan {
		let slot = &mut held[usize::from(split.consume.get())];
		assert!(*slot > 0, "the plan consumes a coin the holding has");
		*slot -= 1;
	}
	let mut coins: Vec<u8> = held
		.iter()
		.enumerate()
		.flat_map(|(raw, count)| core::iter::repeat_n(raw as u8, *count as usize))
		.collect();
	coins.extend(plan.iter().flat_map(|split| split.produce.iter().map(|e| e.get())));
	coins
}

fn new_coins(plan: &[Split]) -> usize {
	plan.iter().map(|split| split.produce.len() - 1).sum()
}

/// Every way to write `2^raw` as a sum of at least two smaller powers of two,
/// as exponents in ascending order.
fn partitions(raw: u8) -> Vec<Vec<u8>> {
	fn fill(remaining: u64, largest: u8, prefix: &mut Vec<u8>, into: &mut Vec<Vec<u8>>) {
		if remaining == 0 {
			let mut parts = prefix.clone();
			parts.reverse();
			into.push(parts);
			return;
		}
		for part in (0..=largest).rev() {
			if 1 << part <= remaining {
				prefix.push(part);
				fill(remaining - (1 << part), part, prefix, into);
				prefix.pop();
			}
		}
	}
	let mut into = Vec::new();
	if raw > 0 {
		fill(1 << raw, raw - 1, &mut Vec::new(), &mut into);
	}
	into
}

/// The fewest new coins any one-level plan needs to make `amount` payable,
/// found by trying every way to split every coin.
fn brute_optimum(amount: u64, counts: &[u32]) -> Option<usize> {
	let coins: Vec<u8> = counts
		.iter()
		.enumerate()
		.flat_map(|(raw, count)| core::iter::repeat_n(raw as u8, *count as usize))
		.collect();
	let choices: Vec<Vec<Option<Vec<u8>>>> = coins
		.iter()
		.map(|coin| core::iter::once(None).chain(partitions(*coin).into_iter().map(Some)).collect())
		.collect();
	let mut best = None;
	let mut pick = vec![0usize; coins.len()];
	loop {
		let mut result = Vec::new();
		let mut cost = 0;
		for (index, coin) in coins.iter().enumerate() {
			match &choices[index][pick[index]] {
				None => result.push(*coin),
				Some(parts) => {
					cost += parts.len() - 1;
					result.extend(parts);
				},
			}
		}
		if best.is_none_or(|known| cost < known) && reachable(amount, &result) {
			best = Some(cost);
		}
		let Some(position) = (0..coins.len()).find(|&index| pick[index] + 1 < choices[index].len())
		else {
			return best;
		};
		pick[position] += 1;
		pick[..position].fill(0);
	}
}

#[test]
fn every_plan_on_small_holdings_is_sufficient_conserving_and_minimal() {
	let params = chain_params();
	for mask in 0u32..32 {
		let counts: Vec<u32> = (0..5).map(|bit| (mask >> bit) & 1).collect();
		let total: u64 = counts.iter().enumerate().map(|(raw, count)| u64::from(*count) << raw).sum();
		for amount in 0..=total {
			let plan = plan_splits(amount, &holding(&counts), &params).expect("the holding covers it");
			for split in &plan {
				let sum: u64 = split.produce.iter().map(|e| e.value()).sum();
				assert_eq!(sum, split.consume.value(), "conservation for {counts:?}, {amount}");
				assert!(split.produce.windows(2).all(|pair| pair[0] <= pair[1]));
			}
			let consumed: Vec<u8> = plan.iter().map(|split| split.consume.get()).collect();
			assert!(consumed.windows(2).all(|pair| pair[0] < pair[1]), "one split per run, ascending");
			assert!(reachable(amount, &applied(&counts, &plan)), "sufficiency for {counts:?}, {amount}");
			assert_eq!(Some(new_coins(&plan)), brute_optimum(amount, &counts), "minimality for {counts:?}, {amount}");
		}
	}
}

#[test]
fn every_plan_on_generated_holdings_is_sufficient() {
	let params = chain_params();
	let mut state = 0x9e37_79b9_u32;
	let mut next = move || {
		state ^= state << 13;
		state ^= state >> 17;
		state ^= state << 5;
		state
	};
	for _ in 0..500 {
		let counts: Vec<u32> = (0..15).map(|_| next() % 3).collect();
		let total: u64 = counts.iter().enumerate().map(|(raw, count)| u64::from(*count) << raw).sum();
		let amount = u64::from(next()) % (total + 1);
		let plan = plan_splits(amount, &holding(&counts), &params).expect("the holding covers it");
		let produced: usize = plan.iter().map(|split| split.produce.len()).sum();
		assert!(plan.iter().all(|split| split.produce.len() <= params.max_split_outputs() as usize));
		assert!(plan.len() <= 7 && produced <= 15 * plan.len());
		let coins = applied(&counts, &plan);
		let greedy_reaches = {
			let mut remaining = amount;
			let mut sorted = coins.clone();
			sorted.sort_unstable_by(|a, b| b.cmp(a));
			for coin in sorted {
				if 1 << coin <= remaining {
					remaining -= 1 << coin;
				}
			}
			remaining == 0
		};
		assert!(greedy_reaches, "sufficiency for {counts:?}, {amount}");
	}
}

#[test]
fn an_amount_above_the_holding_is_not_coverable() {
	assert_eq!(plan_splits(8, &holding(&[1, 1, 1]), &chain_params()), None);
}

#[test]
fn a_payable_amount_needs_no_split() {
	assert_eq!(plan_splits(5, &holding(&[1, 0, 1]), &chain_params()), Some(Vec::new()));
}

#[test]
fn the_plan_splits_one_coin_per_run_of_forced_exponents() {
	let params = chain_params();
	// 27 cents from a 4 cent and a 32 cent coin: exponents 1, 2, 4 and 5 are forced.
	let plan = plan_splits(27, &holding(&[0, 0, 1, 0, 0, 1]), &params).unwrap();
	assert_eq!(raw_plan(&plan), [(2, vec![0, 0, 1]), (5, vec![3, 3, 4])]);
	// One coin of 1 and one of 2^14, paying 2^13 + 1: only exponent 14 is forced.
	let mut counts = vec![0; 15];
	counts[0] = 1;
	counts[14] = 1;
	let plan = plan_splits((1 << 13) + 1, &holding(&counts), &params).unwrap();
	assert_eq!(raw_plan(&plan), [(14, vec![13, 13])]);
	// Coins of 2, 2^13 and 2^14, paying 2^14 + 1: only exponent 1 is forced.
	let mut counts = vec![0; 15];
	counts[1] = 1;
	counts[13] = 1;
	counts[14] = 1;
	let plan = plan_splits((1 << 14) + 1, &holding(&counts), &params).unwrap();
	assert_eq!(raw_plan(&plan), [(1, vec![0, 0])]);
	// $1.27 from 2, 4, ..., 32 and two 64 cent coins: one run from 1 to 6.
	let plan = plan_splits(127, &holding(&[0, 1, 1, 1, 1, 1, 2]), &params).unwrap();
	assert_eq!(raw_plan(&plan), [(6, vec![0, 0, 1, 2, 3, 4, 5])]);
}
