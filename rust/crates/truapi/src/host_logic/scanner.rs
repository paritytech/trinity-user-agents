//! What a product may ask the host scanner, and which scanned codes answer it.
//!
//! The core checks requests and host answers with these rules, and native hosts
//! filter codes in the viewfinder with [`ScanFilter`], so every host accepts
//! the same codes.

use std::{collections::BTreeSet, sync::Mutex};

use truapi::latest::{CodeFormat, HostScannerScanRequest};

/// Longest prefix a product may require, in bytes of UTF-8.
pub const MAX_PREFIX_BYTES: usize = 256;
/// Longest hint a product may show, in Unicode scalar values.
pub const MAX_HINT_CHARS: usize = 80;

/// Why `request` cannot be shown, if it cannot.
pub fn validate_request(request: &HostScannerScanRequest) -> Result<(), String> {
	if request.formats.is_empty() {
		return Err("formats is empty".into());
	}
	if request
		.formats
		.iter()
		.enumerate()
		.any(|(index, format)| request.formats[..index].contains(format))
	{
		return Err("formats names a format twice".into());
	}
	if let Some(prefix) = &request.prefix &&
		prefix.len() > MAX_PREFIX_BYTES
	{
		return Err(format!("prefix is longer than {MAX_PREFIX_BYTES} bytes"));
	}
	if let Some(hint) = &request.hint {
		if hint.chars().count() > MAX_HINT_CHARS {
			return Err(format!("hint is longer than {MAX_HINT_CHARS} characters"));
		}
		if hint.chars().any(breaks_the_line) {
			return Err("hint must be one plain line".into());
		}
	}
	Ok(())
}

/// Characters that would let a hint break out of its line, flip its
/// direction, or hide text under the host's title.
fn breaks_the_line(character: char) -> bool {
	character.is_control() ||
		matches!(
			character,
			'\u{2028}'
				| '\u{2029}'
				| '\u{061C}'
				| '\u{200B}'..='\u{200F}'
				| '\u{202A}'..='\u{202E}'
				| '\u{2060}'..='\u{2069}'
				| '\u{FEFF}'
				| '\u{FFF9}'..='\u{FFFB}'
		)
}

/// Whether a code of `format` reading `text` answers `request`.
///
/// A pairing request never does, whatever the request asks for: whoever
/// answers one pairs with the device that showed it.
pub fn accepts(request: &HostScannerScanRequest, format: CodeFormat, text: &str) -> bool {
	!is_pairing_request(text) &&
		request.formats.contains(&format) &&
		request.prefix.as_deref().is_none_or(|prefix| {
			text.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix))
		})
}

/// Proposal version tags wallets read: V1, which the core no longer sends, and
/// V2.
const PROPOSAL_VERSIONS: [u8; 2] = [0, 1];
/// A version tag followed by the host's two 32-byte device keys, the start of
/// every proposal version.
const MIN_PROPOSAL_BYTES: usize = 1 + 32 + 32;

/// Whether `text` carries a pairing handshake in a form some wallet reads: as
/// any `handshake=` value or bare, in hex with or without `0x`. Matched by
/// shape rather than decoded, so a wallet more lenient than the core still
/// never receives one through a product.
fn is_pairing_request(text: &str) -> bool {
	let handshake_values = text
		.split("handshake=")
		.skip(1)
		.map(|rest| rest.split(['&', '#']).next().unwrap_or_default());
	core::iter::once(text).chain(handshake_values).any(is_handshake_proposal)
}

fn is_handshake_proposal(payload: &str) -> bool {
	let payload = payload.trim();
	let hex = payload
		.strip_prefix("0x")
		.or_else(|| payload.strip_prefix("0X"))
		.unwrap_or(payload);
	hex::decode(hex).is_ok_and(|bytes| {
		bytes.len() >= MIN_PROPOSAL_BYTES && PROPOSAL_VERSIONS.contains(&bytes[0])
	})
}

/// What a host does with one code the camera read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ScanVerdict {
	/// Close the viewfinder and answer with this code.
	Accept,
	/// Briefly tell the user this code is not for the product, and keep scanning.
	NotForThisProduct,
	/// Do nothing and keep scanning.
	Ignore,
}

/// Decides, code by code, what the viewfinder does for one scan.
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Object))]
pub struct ScanFilter {
	request: HostScannerScanRequest,
	state: Mutex<FilterState>,
}

#[derive(Default)]
struct FilterState {
	accepted: bool,
	refused: BTreeSet<(u8, String)>,
}

#[cfg_attr(not(target_arch = "wasm32"), uniffi::export)]
impl ScanFilter {
	/// A filter for one scan of `request`.
	#[cfg_attr(not(target_arch = "wasm32"), uniffi::constructor)]
	pub fn new(request: HostScannerScanRequest) -> Self {
		Self { request, state: Mutex::default() }
	}

	/// What to do with a code the camera read.
	pub fn observe(&self, format: CodeFormat, text: String) -> ScanVerdict {
		let mut state = self.state.lock().expect("scan filter mutex poisoned");
		if state.accepted {
			return ScanVerdict::Ignore;
		}
		if accepts(&self.request, format, &text) {
			state.accepted = true;
			return ScanVerdict::Accept;
		}
		if state.refused.insert((format as u8, text)) {
			ScanVerdict::NotForThisProduct
		} else {
			ScanVerdict::Ignore
		}
	}
}

#[cfg(test)]
mod tests;
