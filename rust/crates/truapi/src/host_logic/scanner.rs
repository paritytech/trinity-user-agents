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
        formats: Vec<CodeFormat>,
        prefix: Option<&str>,
        hint: Option<&str>,
    ) -> HostScannerScanRequest {
        HostScannerScanRequest {
            formats,
            prefix: prefix.map(str::to_owned),
            hint: hint.map(str::to_owned),
        }
    }

    fn qr(prefix: Option<&str>, hint: Option<&str>) -> HostScannerScanRequest {
        request(vec![CodeFormat::Qr], prefix, hint)
    }

    fn receipts() -> HostScannerScanRequest {
        qr(Some("https://greenmarket.example/r/"), None)
    }

    #[test]
    fn a_request_must_name_a_format() {
        // With no formats the host, not the product, would decide what comes back.
        assert!(validate_request(&request(vec![], None, None)).is_err());
        assert!(validate_request(&qr(None, None)).is_ok());
    }

    #[test]
    fn a_format_may_be_named_only_once() {
        // The filter checks the list on every camera frame, so it stays as
        // short as the formats themselves.
        assert!(
            validate_request(&request(vec![CodeFormat::Qr, CodeFormat::Qr], None, None)).is_err()
        );
        assert!(
            validate_request(&request(
                vec![CodeFormat::Qr, CodeFormat::Ean13],
                None,
                None
            ))
            .is_ok()
        );
    }

    #[test]
    fn the_hint_is_counted_in_unicode_scalars() {
        // TypeScript products count with `[...hint].length`, which agrees with this.
        assert!(validate_request(&qr(None, Some(&"é".repeat(MAX_HINT_CHARS)))).is_ok());
        assert!(validate_request(&qr(None, Some(&"a".repeat(MAX_HINT_CHARS + 1)))).is_err());
    }

    #[test]
    fn the_hint_stays_one_plain_line() {
        // The hint sits under the host's title, so text that breaks the line or
        // flips its direction could pass for a host instruction.
        for refused in [
            "scan\nto sign in",
            "tab\there",
            "line\u{2028}separator",
            "para\u{2029}separator",
            "\u{202E}reversed",
            "\u{2066}isolate",
            "\u{200F}mark",
            "\u{061C}arabic letter mark",
            "zero\u{200B}width",
            "word\u{2060}joiner",
            "\u{FEFF}byte order mark",
            "\u{FFF9}annotation",
        ] {
            assert!(
                validate_request(&qr(None, Some(refused))).is_err(),
                "{refused:?} was allowed"
            );
        }
        assert!(validate_request(&qr(None, Some("Point at the receipt's QR code"))).is_ok());
    }

    #[test]
    fn the_prefix_is_counted_in_bytes() {
        assert!(validate_request(&qr(Some(&"a".repeat(MAX_PREFIX_BYTES)), None)).is_ok());
        let over = "é".repeat(MAX_PREFIX_BYTES / 2 + 1);
        assert!(validate_request(&qr(Some(&over), None)).is_err());
    }

    #[test]
    fn a_code_is_accepted_only_in_a_named_format() {
        assert!(accepts(&qr(None, None), CodeFormat::Qr, "anything"));
        assert!(!accepts(
            &qr(None, None),
            CodeFormat::Ean13,
            "4006381333931"
        ));
    }

    #[test]
    fn the_prefix_ignores_letter_case_and_nothing_else() {
        // QR codes often store URLs in capitals to fit more in the code.
        assert!(accepts(
            &receipts(),
            CodeFormat::Qr,
            "https://greenmarket.example/r/BAG6"
        ));
        assert!(accepts(
            &receipts(),
            CodeFormat::Qr,
            "HTTPS://GREENMARKET.EXAMPLE/R/BAG6"
        ));
        assert!(!accepts(
            &receipts(),
            CodeFormat::Qr,
            "polkadotapp://pair?handshake=00"
        ));
        assert!(!accepts(
            &receipts(),
            CodeFormat::Qr,
            "https://greenmarket.example"
        ));
    }

    #[test]
    fn a_prefix_that_ends_inside_a_character_does_not_panic() {
        // A byte-length cut through a multi-byte character must refuse, not crash.
        assert!(!accepts(&qr(Some("ab"), None), CodeFormat::Qr, "aé"));
    }

    #[test]
    fn the_filter_accepts_once() {
        // The viewfinder sees the same code on many frames before it closes.
        let filter = ScanFilter::new(receipts());
        let receipt = "https://greenmarket.example/r/BAG6".to_owned();
        assert_eq!(
            filter.observe(CodeFormat::Qr, receipt.clone()),
            ScanVerdict::Accept
        );
        assert_eq!(filter.observe(CodeFormat::Qr, receipt), ScanVerdict::Ignore);
    }

    #[test]
    fn the_filter_names_each_wrong_code_once_per_scan() {
        // A poster with two codes on it is read alternately, frame after frame.
        // The user should hear about each once, not on every frame.
        let filter = ScanFilter::new(receipts());
        let poster = "https://elsewhere.example".to_owned();
        let pairing = "polkadotapp://pair?handshake=00".to_owned();
        for (code, expected) in [
            (&poster, ScanVerdict::NotForThisProduct),
            (&pairing, ScanVerdict::NotForThisProduct),
            (&poster, ScanVerdict::Ignore),
            (&pairing, ScanVerdict::Ignore),
        ] {
            assert_eq!(filter.observe(CodeFormat::Qr, code.clone()), expected);
        }
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
            assert!(!accepts(&qr(None, None), CodeFormat::Qr, text), "{text}");
            assert!(!accepts(&qr(Some("polkadotapp://pair?"), None), CodeFormat::Qr, text));
        }
        assert_eq!(
            ScanFilter::new(qr(None, None)).observe(CodeFormat::Qr, link),
            ScanVerdict::NotForThisProduct
        );
    }

    #[test]
    fn the_filter_still_accepts_after_refusing() {
        let filter = ScanFilter::new(receipts());
        filter.observe(CodeFormat::Qr, "https://elsewhere.example".to_owned());
        assert_eq!(
            filter.observe(CodeFormat::Qr, "https://greenmarket.example/r/1".to_owned()),
            ScanVerdict::Accept
        );
    }
}
