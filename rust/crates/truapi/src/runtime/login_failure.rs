//! Classification of login failures into [`LoginFailureKind`].
//!
//! A wallet reports why it refused pairing as prose, over
//! `EncryptedResponse::Failed` on the inter-host wire, so the core recovers the
//! discriminant here instead of leaving every host to pattern-match the text.
//!
//! The wording is the wallet's, not this workspace's: the reason travels from
//! an external signing host and no producer in this repo emits it. The rule is
//! therefore deliberately broad, and at least as broad as the host-side regexes
//! it replaces — a host that drops its own matching for `kind` must not lose
//! fast-fail. The tests cover both this workspace's `SlotError` renderings and
//! wordings observed from real wallets.

use crate::platform::LoginFailureKind;

/// Whether `text` reports an allowance period with no slot left.
///
/// The one rule for that question: the signing host reads it to rotate an
/// exhausted auto-managed account, and `classify_login_failure` reads it to
/// type a wallet's refusal. It mirrors the
/// [`SlotError`](crate::runtime::statement_allowance::slot::SlotError)
/// `Display` strings, which a test beside those strings pins, and lives here
/// rather than beside them because the wasm32 host classifies login failures
/// without compiling the allowance allocator.
///
/// The rule matches on "no free" and "slot" instead of a full rendering because
/// the same fact also arrives as prose from an external wallet, whose wording
/// this workspace does not control, and because a caller that misses the case
/// retries something that will not succeed until the period rolls over.
/// Callers that need certainty must match `SlotError` itself.
pub fn reports_exhausted_period(text: &str) -> bool {
	let text = text.to_ascii_lowercase();
	text.contains("no free") && text.contains("slot")
}

/// Recover the failure kind from a wallet-reported reason.
pub fn classify_login_failure(reason: &str) -> LoginFailureKind {
	if reports_exhausted_period(reason) {
		return LoginFailureKind::NoFreeAllowanceSlots;
	}
	LoginFailureKind::Other
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::runtime::statement_allowance::slot::SlotError;

	#[test]
	fn exhausted_allowance_periods_are_recognized_from_slot_error_text() {
		for error in [
			SlotError::NoFreeStatementStoreSlot { period: 7, max: 8 },
			SlotError::NoFreeLongTermStorageSlot { period: 7, max: 8 },
		] {
			assert_eq!(
				classify_login_failure(&error.to_string()),
				LoginFailureKind::NoFreeAllowanceSlots,
				"`{error}` must classify as an exhausted allowance period"
			);
		}
	}

	#[test]
	fn wallet_wordings_seen_in_the_wild_are_recognized() {
		for reason in [
			"no free statement store slot in period 20486 (max 8)",
			"No free slots available (limit=8)",
			"no free slot in period 20486",
		] {
			assert_eq!(
				classify_login_failure(reason),
				LoginFailureKind::NoFreeAllowanceSlots,
				"`{reason}` must classify as an exhausted allowance period"
			);
		}
	}

	#[test]
	fn other_slot_failures_are_not_reported_as_exhausted_periods() {
		for error in [
			SlotError::LongTermStoragePeriodDurationZero,
			SlotError::ReplacementRefused { period: 7, seq: 3 },
			SlotError::FreeSlotsAwaitingSubmission { period: 7 },
			SlotError::MissingChainTimestamp,
			SlotError::RegistrationVerificationMismatch {
				block_hash: "0xabc".to_string(),
				period: 7,
				seq: 3,
			},
		] {
			assert_eq!(
				classify_login_failure(&error.to_string()),
				LoginFailureKind::Other,
				"`{error}` is not an exhausted allowance period"
			);
		}
	}

	#[test]
	fn unrelated_reasons_are_other() {
		for reason in [
			"",
			"user rejected pairing",
			"pairing statement-store subscribe failed: timeout",
			"The operation couldn't be completed. (SubstrateSdk.JSONRPCError error 1.)",
		] {
			assert_eq!(
				classify_login_failure(reason),
				LoginFailureKind::Other,
				"`{reason}` is not an exhausted allowance period"
			);
		}
	}
}
