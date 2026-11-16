//! A5 · Split planning: which coins to split, and into what, so that some
//! subset of the coins sums exactly to an amount.
//!
//! The plan is the one contract 5 defines: one split per run of forced
//! exponents. `docs/design/coinage/formal-split-algo.md` proves that it
//! creates the fewest new coins (Theorems A and B), and
//! `intuitive-split-algo.md` explains why.

use super::{ChainParams, Exponent};

/// One coin of exponent `consume`, split into coins of the exponents in
/// `produce`, which are listed in ascending order and sum to its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split {
	/// The exponent of the coin the split consumes.
	pub consume: Exponent,
	/// The exponents of the coins it creates, ascending.
	pub produce: Vec<Exponent>,
}

/// The plan that makes `amount` payable from coins counted by `counts`, or
/// none when the coins are worth less than `amount`.
///
/// `counts` gives the number of splittable coins of each exponent; an
/// exponent may appear more than once, and its counts add up. The planner sees
/// counts, never coins: a split names the exponent of the coin it consumes,
/// and the caller chooses which coin of that exponent it is. The splits are
/// listed in ascending order of the exponent they consume, and none consumes
/// another's output, so they can all be submitted at once.
pub fn plan_splits(amount: u64, counts: &[(Exponent, u32)], params: &ChainParams) -> Option<Vec<Split>> {
	let largest = params.largest_exponent().get();
	let mut held = vec![0u128; usize::from(largest) + 1];
	for (exponent, count) in counts {
		let slot = held.get_mut(usize::from(exponent.get()))?;
		*slot += u128::from(*count);
	}
	let total: u128 = held.iter().enumerate().map(|(exponent, count)| count << exponent).sum();
	if u128::from(amount) > total {
		return None;
	}

	let mut below = 0u128;
	let forced: Vec<u8> = (1..=largest)
		.filter(|&j| {
			below += held[usize::from(j) - 1] << (j - 1);
			u128::from(amount % (1u64 << j)) > below
		})
		.collect();
	Some(runs(&forced).map(|(lowest, highest)| run_split(lowest, highest)).collect())
}

/// The maximal runs of consecutive exponents in `forced`, which is ascending,
/// as their lowest and highest exponent.
fn runs(forced: &[u8]) -> impl Iterator<Item = (u8, u8)> + '_ {
	forced
		.chunk_by(|lower, upper| lower + 1 == *upper)
		.map(|run| (run[0], run[run.len() - 1]))
}

/// The split for the run from `lowest` to `highest`: one coin of `highest`
/// into two coins of `lowest - 1` and one of each exponent from `lowest` to
/// `highest - 1`.
fn run_split(lowest: u8, highest: u8) -> Split {
	let exponent = |raw: u8| Exponent::new(raw).expect("a run lies within the legal exponents");
	let produce = core::iter::once(lowest - 1)
		.chain(lowest - 1..highest)
		.map(exponent)
		.collect();
	Split { consume: exponent(highest), produce }
}

#[cfg(test)]
mod tests;
