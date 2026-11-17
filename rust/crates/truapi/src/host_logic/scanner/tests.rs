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
	assert!(valid(&[CodeFormat::Qr, CodeFormat::Ean13], None, Some("Point at the receipt")));
	assert!(valid(qr, Some(&"a".repeat(MAX_PREFIX_BYTES)), Some(&"é".repeat(MAX_HINT_CHARS))));
	assert!(!valid(&[], None, None));
	assert!(!valid(&[CodeFormat::Qr, CodeFormat::Qr], None, None));
	assert!(!valid(qr, Some(&"é".repeat(MAX_PREFIX_BYTES / 2 + 1)), None));
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
		(CodeFormat::Ean13, "https://greenmarket.example/r/BAG6", false),
		(CodeFormat::Qr, "https://greenmarket.example", false),
		(CodeFormat::Qr, "https://elsewhere.example/r/", false),
	] {
		assert_eq!(accepts(&receipts, format, text), accepted, "{text}");
	}
	// A byte-length cut through a multi-byte character refuses, not panics.
	assert!(!accepts(&request(&[CodeFormat::Qr], Some("ab"), None), CodeFormat::Qr, "aé"));
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
	let handshake = link.split_once("?handshake=").unwrap().1;
	// Wallets read these forms too, and a V1 proposal starts with tag 0.
	let v1_handshake = format!("00{}", "07".repeat(70));
	for text in [
		link.clone(),
		link.replacen("polkadotapp", "otherwallet", 1),
		link.replacen("?handshake=", "?handshake=0x", 1),
		link.replacen("?handshake=", "?source=poster&handshake=", 1),
		format!("{link}&source=poster"),
		format!("polkadotapp://pair?handshake={v1_handshake}"),
		handshake.to_owned(),
		format!("0x{handshake}"),
	] {
		for prefix in [None, Some("polkadotapp://pair?")] {
			let any_qr = request(&[CodeFormat::Qr], prefix, None);
			assert!(!accepts(&any_qr, CodeFormat::Qr, &text), "{text}");
		}
	}
	// Too short to hold a proposal's keys, so a product may receive it.
	let account = format!("0x{}", "01".repeat(32));
	assert!(accepts(&request(&[CodeFormat::Qr], None, None), CodeFormat::Qr, &account));
}

#[test]
fn the_filter_accepts_once_and_names_each_wrong_code_once() {
	// The camera reads the same codes frame after frame, and a poster with
	// two codes is read in turn. The user hears about each code once.
	let filter = ScanFilter::new(request(&[CodeFormat::Qr], Some(RECEIPTS), None));
	let observe = |text: &str| filter.observe(CodeFormat::Qr, text.to_owned());
	assert_eq!(observe("https://elsewhere.example"), ScanVerdict::NotForThisProduct);
	assert_eq!(observe("https://other.example"), ScanVerdict::NotForThisProduct);
	assert_eq!(observe("https://elsewhere.example"), ScanVerdict::Ignore);
	assert_eq!(observe("https://greenmarket.example/r/1"), ScanVerdict::Accept);
	assert_eq!(observe("https://greenmarket.example/r/1"), ScanVerdict::Ignore);
}
