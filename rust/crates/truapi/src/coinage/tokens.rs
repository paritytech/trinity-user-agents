//! D1 · Unload-token supply: which of the period's free unload tokens the
//! layer may still spend.
//!
//! Unloading recycler entries costs one free token of the user's personhood
//! credential per unload transaction. A token's state is read from the
//! records that present it and from what F1 observed; nothing stores it.

use super::{
	ChainParams, Snapshot,
	ledger::{Read, Subject, TokenId},
};
use crate::durable::{DurableTxFailureKind, DurableTxStatus};

/// Whether a token identity has been spent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenState {
	/// Some transaction presenting it reached the chain, whether or not its
	/// dispatch succeeded, or the chain shows it consumed.
	Applied,
	/// Every transaction presenting it failed without reaching the chain.
	NotApplied,
	/// Neither is known yet. It blocks the token as `Applied` does.
	Unknown,
}

/// The current period's supply of free tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Supply {
	/// The period.
	pub period: u32,
	/// The tokens of a period, as F1 last read them for the user's
	/// collection; none before the first read.
	pub allowance: Option<u32>,
	/// The free tokens below which recycling the policy asks for stops
	/// (invariant 2).
	pub reserve: u32,
	/// Each slot's state, by slot.
	pub states: Vec<TokenState>,
	/// Whether each slot is presented by a transaction that has not failed.
	pub held: Vec<bool>,
}

impl Supply {
	/// The slots that are not applied and not held, lowest first.
	pub fn free(&self) -> impl Iterator<Item = u32> + '_ {
		(0..self.allowance.unwrap_or(0)).filter(|slot| {
			let index = *slot as usize;
			self.states[index] == TokenState::NotApplied && !self.held[index]
		})
	}
}

/// The period that contains `now`, in Unix seconds.
pub fn period_of(now: u64, params: &ChainParams) -> u32 {
	let length = params.token_period().as_secs().max(1);
	u32::try_from(now / length).unwrap_or(u32::MAX)
}

/// A token identity's state, from the records that present it, their
/// statuses, and F1's observation.
pub fn token_state(token: TokenId, view: &Snapshot) -> TokenState {
	let observed_consumed = matches!(
		view.observation(&Subject::Token(token)).and_then(|observation| observation.value.as_ref()),
		Some(Read::Token { consumed: true })
	);
	if observed_consumed {
		return TokenState::Applied;
	}
	let mut every_failed_unsent = true;
	for presenter in view.registrations().presenters(&token) {
		match presenter.state.status {
			DurableTxStatus::PendingSuccess { .. } |
			DurableTxStatus::FinalizedSuccess { .. } |
			DurableTxStatus::Failure(DurableTxFailureKind::DispatchFailed) => return TokenState::Applied,
			DurableTxStatus::Failure(_) => {},
			DurableTxStatus::PendingSubmission | DurableTxStatus::Pending => every_failed_unsent = false,
		}
	}
	if every_failed_unsent { TokenState::NotApplied } else { TokenState::Unknown }
}

/// The supply of the period that contains `now`.
pub fn supply(now: u64, view: &Snapshot, params: &ChainParams) -> Supply {
	let period = period_of(now, params);
	let allowance = view.unload_allowance();
	let slots = allowance.unwrap_or(0);
	let reserve = slots * u32::from(params.config().reserve_share) / 100;
	let tokens: Vec<TokenId> = (0..slots).map(|slot| TokenId { period, slot }).collect();
	let states = tokens.iter().map(|token| token_state(*token, view)).collect();
	let held = tokens
		.iter()
		.map(|token| {
			view.registrations()
				.presenters(token)
				.any(|presenter| !matches!(presenter.state.status, DurableTxStatus::Failure(_)))
		})
		.collect();
	Supply { period, allowance, reserve, states, held }
}

/// `needed` token identities, the free slots lowest first (invariant 3), or
/// none when the supply is short: a plan short of even one token is refused
/// whole (invariant 1).
pub fn resolve(needed: u32, supply: &Supply) -> Option<Vec<TokenId>> {
	let tokens: Vec<TokenId> =
		supply.free().take(needed as usize).map(|slot| TokenId { period: supply.period, slot }).collect();
	(tokens.len() == needed as usize).then_some(tokens)
}

/// Whether the free tokens are no more than the reserve, which stops the
/// recycling the policy asks for (invariant 2). An allowance not yet read
/// is not evidence of an exhausted one, so it is not running low.
pub fn running_low(supply: &Supply) -> bool {
	supply.allowance.is_some() && supply.free().count() <= supply.reserve as usize
}

#[cfg(test)]
mod tests;
