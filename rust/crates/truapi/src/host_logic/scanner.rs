//! What a product may ask the host scanner, and which scanned codes answer it.
//!
//! The core checks requests and host answers with these rules, and native hosts
//! filter codes in the viewfinder with [`ScanFilter`], so every host accepts
//! the same codes.

use std::collections::BTreeSet;
use std::sync::Mutex;

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
    if let Some(prefix) = &request.prefix
        && prefix.len() > MAX_PREFIX_BYTES
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
    character.is_control()
        || matches!(
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
    !is_pairing_request(text)
        && request.formats.contains(&format)
        && request.prefix.as_deref().is_none_or(|prefix| {
            text.get(..prefix.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
        })
}

/// Whether the core would accept `text` as a pairing request, with or without
/// the `pair` link around the handshake.
fn is_pairing_request(text: &str) -> bool {
    crate::host_logic::sso::pairing::decode_pairing_deeplink(text).is_ok()
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
        Self {
            request,
            state: Mutex::default(),
        }
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
mod tests {
    use super::*;

    fn request(
        formats: &[CodeFormat],
        prefix: Option<&str>,
        hint: Option<&str>,
    ) -> HostScannerScanRequest {
        HostScannerScanRequest {
            formats: formats.to_vec(),
            prefix: prefix.map(str::to_owned),
            hint: hint.map(str::to_owned),
        }
    }

    fn valid(formats: &[CodeFormat], prefix: Option<&str>, hint: Option<&str>) -> bool {
        validate_request(&request(formats, prefix, hint)).is_ok()
    }

    const RECEIPTS: &str = "https://greenmarket.example/r/";

    #[test]
    fn a_request_names_each_format_once_and_keeps_its_hint_to_one_plain_line() {
        // With no formats the host would decide what comes back. The hint sits
        // under the host's title, so text that breaks the line, flips its
        // direction or hides itself could pass for host text. TypeScript counts
        // a hint with `[...hint].length`, which agrees with the scalar count.
        let qr = &[CodeFormat::Qr];
        assert!(valid(
            &[CodeFormat::Qr, CodeFormat::Ean13],
            None,
            Some("Point at the receipt")
        ));
        assert!(valid(
            qr,
            Some(&"a".repeat(MAX_PREFIX_BYTES)),
            Some(&"é".repeat(MAX_HINT_CHARS))
        ));
        assert!(!valid(&[], None, None));
        assert!(!valid(&[CodeFormat::Qr, CodeFormat::Qr], None, None));
        assert!(!valid(
            qr,
            Some(&"é".repeat(MAX_PREFIX_BYTES / 2 + 1)),
            None
        ));
        assert!(!valid(qr, None, Some(&"a".repeat(MAX_HINT_CHARS + 1))));
        for refused in [
            "scan\nto sign in",
            "line\u{2028}separator",
            "para\u{2029}separator",
            "\u{202E}reversed",
            "\u{2066}isolate",
            "\u{200F}mark",
            "\u{061C}arabic letter mark",
            "\u{FEFF}byte order mark",
            "\u{FFF9}annotation",
        ] {
            assert!(!valid(qr, None, Some(refused)), "{refused:?}");
        }
    }

    #[test]
    fn a_code_must_have_a_named_format_and_the_prefix_in_any_letter_case() {
        // QR codes often store URLs in capitals to fit more in the code.
        let receipts = request(&[CodeFormat::Qr], Some(RECEIPTS), None);
        for (format, text, accepted) in [
            (CodeFormat::Qr, "https://greenmarket.example/r/BAG6", true),
            (CodeFormat::Qr, "HTTPS://GREENMARKET.EXAMPLE/R/BAG6", true),
            (
                CodeFormat::Ean13,
                "https://greenmarket.example/r/BAG6",
                false,
            ),
            (CodeFormat::Qr, "https://greenmarket.example", false),
            (CodeFormat::Qr, "https://elsewhere.example/r/", false),
        ] {
            assert_eq!(accepts(&receipts, format, text), accepted, "{text}");
        }
        // A byte-length cut through a multi-byte character refuses, not panics.
        assert!(!accepts(
            &request(&[CodeFormat::Qr], Some("ab"), None),
            CodeFormat::Qr,
            "aé"
        ));
    }

    #[test]
    fn a_pairing_request_never_reaches_a_product() {
        // Whoever answers a pairing link pairs with the device that showed it,
        // so no request may receive one, not even one asking for its prefix.
        let (config, _) = crate::test_support::runtime_config("greenmarket.dot");
        let link = crate::host_logic::sso::pairing::build_pairing_deeplink(
            "polkadotapp",
            [1; 32],
            [2; 32],
            &config,
        );
        let other_scheme = link.replacen("polkadotapp", "otherwallet", 1);
        let bare_handshake = link.split_once("?handshake=").unwrap().1.to_owned();
        for text in [&link, &other_scheme, &bare_handshake] {
            for prefix in [None, Some("polkadotapp://pair?")] {
                let any_qr = request(&[CodeFormat::Qr], prefix, None);
                assert!(!accepts(&any_qr, CodeFormat::Qr, text), "{text}");
            }
        }
    }

    #[test]
    fn the_filter_accepts_once_and_names_each_wrong_code_once() {
        // The camera reads the same codes frame after frame, and a poster with
        // two codes is read in turn. The user hears about each code once.
        let filter = ScanFilter::new(request(&[CodeFormat::Qr], Some(RECEIPTS), None));
        let observe = |text: &str| filter.observe(CodeFormat::Qr, text.to_owned());
        assert_eq!(
            observe("https://elsewhere.example"),
            ScanVerdict::NotForThisProduct
        );
        assert_eq!(
            observe("https://other.example"),
            ScanVerdict::NotForThisProduct
        );
        assert_eq!(observe("https://elsewhere.example"), ScanVerdict::Ignore);
        assert_eq!(
            observe("https://greenmarket.example/r/1"),
            ScanVerdict::Accept
        );
        assert_eq!(
            observe("https://greenmarket.example/r/1"),
            ScanVerdict::Ignore
        );
    }
}
