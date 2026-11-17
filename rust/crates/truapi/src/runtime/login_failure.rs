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
mod tests;
