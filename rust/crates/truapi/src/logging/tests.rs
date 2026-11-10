use super::*;
use std::sync::Mutex;

/// Both sigils a `tracing` field can use have to redact, or the safe
/// spelling is the one the author has to remember.
#[test]
fn an_account_prints_as_a_handle_under_either_sigil() {
	let account = [0xab, 0xcd, 0xef, 0x01, 0x23, 0x45];

	assert_eq!(Account::new(&account).to_string(), "abcdef01");
	assert_eq!(format!("{:?}", Account::new(&account)), "abcdef01");
}

/// The whole account is still reachable, by a name that says so.
#[test]
fn an_account_names_itself_in_full_only_when_asked() {
	let account = [0xab, 0xcd, 0xef, 0x01, 0x23, 0x45];

	assert_eq!(Account::new(&account).in_full(), "abcdef012345");
}

/// An account shorter than the handle is printed whole rather than
/// panicking on the slice.
#[test]
fn a_short_account_does_not_panic() {
	assert_eq!(Account::new(&[0xab, 0xcd]).to_string(), "abcd");
	assert_eq!(Account::new(&[]).to_string(), "");
}

/// A foreign exception description can be several lines, and a detail the
/// sink forwards is one.
#[test]
fn host_text_is_written_on_one_line() {
	let thrown = "NSError Domain=Host\nCode=3\r\n\tUserInfo={}";

	assert_eq!(
		HostText::new(thrown).to_string(),
		r#""NSError Domain=Host\nCode=3\r\n\tUserInfo={}""#
	);
}

/// `char::is_control` is category Cc alone, so the separators and the
/// bidirectional overrides have to be escaped by something that knows
/// about them.
#[test]
fn host_text_escapes_what_is_control_misses() {
	for (raw, escaped) in [
		("a\u{2028}b", r#""a\u{2028}b""#),
		("a\u{2029}b", r#""a\u{2029}b""#),
		("safe\u{202e}txet", r#""safe\u{202e}txet""#),
		("a\u{200b}b", r#""a\u{200b}b""#),
		("a\u{feff}b", r#""a\u{feff}b""#),
	] {
		assert!(!raw.chars().any(char::is_control), "{raw:?} would need no help");
		assert_eq!(HostText::new(raw).to_string(), escaped);
	}
}

/// A detail is `name=value` pairs joined by commas, so unquoted host text
/// can be read as the fields beside it.
#[test]
fn host_text_cannot_forge_a_field() {
	let thrown = "denied, user=admin, level=error";

	assert_eq!(HostText::new(thrown).to_string(), r#""denied, user=admin, level=error""#);
}

/// One foreign string must not crowd the fields beside it out of the
/// detail the sink forwards.
#[test]
fn host_text_stops_at_the_bound() {
	let thrown = "e".repeat(HOST_TEXT_MAX_CHARS + 20);

	let written = HostText::new(&thrown).to_string();

	// The bound counts the escaped text, with the quotes and the mark on
	// top of it.
	assert_eq!(written.chars().count(), HOST_TEXT_MAX_CHARS + 3);
	assert!(written.ends_with("\u{2026}"), "{written}");
}

/// An escape is written whole or not at all, so the bound cannot leave a
/// half-written `\u{...}` behind.
#[test]
fn host_text_does_not_cut_an_escape_in_half() {
	// Each separator escapes to eight characters, so the bound falls
	// inside one of them rather than between two.
	let thrown = "\u{2028}".repeat(30);

	let written = HostText::new(&thrown).to_string();

	assert_eq!(written.matches(r"\u{2028}").count(), HOST_TEXT_MAX_CHARS / 8);
	assert!(written.ends_with("\u{2026}"), "{written}");
}

/// Text already within the bound is written as the host wrote it, with no
/// mark suggesting something was dropped.
#[test]
fn host_text_within_the_bound_is_untouched() {
	assert_eq!(HostText::new("keychain unavailable").to_string(), r#""keychain unavailable""#);
	assert_eq!(format!("{:?}", HostText::new("keychain unavailable")), r#""keychain unavailable""#);
}

/// The bound counts characters, so text under it in characters and over
/// it in bytes is complete and must not be marked as cut.
#[test]
fn host_text_bounds_characters_not_bytes() {
	let under_in_chars_over_in_bytes = "\u{e9}".repeat(100);
	assert!(under_in_chars_over_in_bytes.len() > HOST_TEXT_MAX_CHARS);

	let written = HostText::new(&under_in_chars_over_in_bytes).to_string();

	assert_eq!(written, format!("\"{under_in_chars_over_in_bytes}\""));
	assert!(!written.ends_with("\u{2026}"), "{written}");
}

/// Text exactly at the bound is complete, so it carries no mark either.
#[test]
fn host_text_exactly_at_the_bound_is_not_marked() {
	let thrown = "e".repeat(HOST_TEXT_MAX_CHARS);

	let written = HostText::new(&thrown).to_string();

	assert_eq!(written, format!("\"{thrown}\""));
}

/// What is kept is bounded the same way as what is logged, so a warning
/// raised about this error later is safe without knowing where the
/// string came from.
#[test]
fn kept_host_text_is_bounded_like_the_line_that_names_it() {
	let thrown = format!("SQLiteFullException\n\tat Foo.bar\u{2028}{}", "e".repeat(400));

	let kept = bound_host_text(&thrown);

	assert!(!kept.chars().any(char::is_control), "{kept}");
	assert!(!kept.contains('\u{2028}'), "{kept}");
	assert_eq!(kept.chars().count(), HOST_TEXT_MAX_CHARS + 1);
	assert!(kept.ends_with('\u{2026}'), "{kept}");
}

/// What is kept is read by a person and returned to a product, so it is
/// not quoted the way the log field is.
#[test]
fn kept_host_text_is_not_quoted() {
	assert_eq!(bound_host_text("keychain unavailable"), "keychain unavailable");
	assert_eq!(HostText::new("keychain unavailable").to_string(), r#""keychain unavailable""#);
}

/// The host sink is process-wide, so a test that installs one has to own it
/// for the whole of its run or a sibling's teardown reads as its own.
static SINK_TESTS: Mutex<()> = Mutex::new(());

/// A host-addressed target the table does not list never reaches a sink,
/// so a marker added without saying what it carries stays on the device.
#[test]
fn an_unregistered_marker_is_refused() {
	let _owned = SINK_TESTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	init();
	set_level(DEFAULT_LEVEL);
	let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
	let recorder = seen.clone();
	let _sink = set_host_sink(Arc::new(move |marker: &str, _detail: &str| {
		recorder.lock().unwrap().push(marker.to_string());
	}));

	tracing::warn!(target: "truapi.not.registered", "a marker nobody declared");
	tracing::warn!(target: "truapi.core.started", "a marker that is declared");

	let seen = seen.lock().unwrap().clone();
	assert!(
		!seen.iter().any(|marker| marker == "truapi.not.registered"),
		"an unregistered marker reached the sink: {seen:?}",
	);
	assert!(
		seen.iter().any(|marker| marker == "truapi.core.started"),
		"the registered marker beside it was dropped too: {seen:?}",
	);
}

/// The floor must stay at `ERROR` so a host that never calls
/// `setLogLevel` still sees protocol violations, and a typo must not be
/// able to silence them.
/// A marked event reaches the host's sink even though the level filter is
/// at its default, which is what an ordinary `warn!` cannot do.
#[test]
fn a_marked_event_reaches_the_host_sink_at_the_default_level() {
	let _owned = SINK_TESTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	init();
	set_level(DEFAULT_LEVEL);
	let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
	let recorder = seen.clone();
	let _sink = set_host_sink(Arc::new(move |marker: &str, detail: &str| {
		recorder.lock().unwrap().push((marker.to_string(), detail.to_string()));
	}));

	tracing::warn!(target: "truapi.chain.follow_budget_reached", "four of four");
	tracing::warn!(target: "truapi::test", "routine crate traffic");

	// Filtered rather than compared whole: other tests in this binary share
	// the subscriber and can raise their own marked events.
	let seen = seen.lock().unwrap().clone();
	assert!(
		seen.contains(&(
			"truapi.chain.follow_budget_reached".to_string(),
			"four of four".to_string()
		)),
		"the sink missed the marked event with the default ERROR filter in place: {seen:?}",
	);
	assert!(
		!seen.iter().any(|(marker, _)| marker == "truapi::test"),
		"an unmarked event reached the host sink: {seen:?}",
	);

	// Leave no sink behind for other tests in this binary.
	*HOST_SINK.write().unwrap() = None;
}

/// A sink that logs is a host writing its own log, so it must not be able
/// to drive the layer back into itself. A global subscriber takes a path
/// with no reentrancy guard of its own, so the layer carries one.
#[test]
fn an_event_raised_inside_the_sink_does_not_re_enter_it() {
	let _owned = SINK_TESTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	init();
	set_level(DEFAULT_LEVEL);
	let depth = Arc::new(Mutex::new(0usize));
	let recorder = depth.clone();
	let _sink = set_host_sink(Arc::new(move |_marker: &str, _detail: &str| {
		*recorder.lock().unwrap() += 1;
		// A plain warning, not a marked one: a host shipping these onward
		// logs on the way, and every warning in the process reaches this
		// sink, so this is the path that actually recurses in production.
		tracing::warn!("raised from inside the sink");
		tracing::info!(target: "truapi.core.started", "and a marked one");
	}));

	tracing::info!(target: "truapi.core.started", "one marked event");

	assert_eq!(*depth.lock().unwrap(), 1, "the event the sink raised re-entered the sink",);

	tracing::warn!("one plain warning");

	assert_eq!(*depth.lock().unwrap(), 2, "the sink re-entered on a plain warning",);

	// Leave no sink behind for other tests in this binary.
	*HOST_SINK.write().unwrap() = None;
}

/// The host filter has no level opinion of its own, so without a hint it
/// would pin the process-wide max level at `TRACE` and cost every `debug!`
/// in every crate the static check that cuts it before any dispatch.
#[test]
fn the_host_filter_does_not_raise_the_process_max_level() {
	init();
	set_level(DEFAULT_LEVEL);
	assert_eq!(LevelFilter::current(), HOST_MARKER_MAX_LEVEL);
}

/// A dependency's warnings are not a host's business, and the reentrancy
/// guard is thread-local, so admitting every crate's would let any of them
/// drive the sink back into itself from a thread the guard cannot see.
#[test]
fn a_warning_from_another_crate_does_not_reach_the_host_sink() {
	let lines = testing::host_sink_lines(|| {
		tracing::warn!(target: "subxt::backend", "a dependency warning");
		tracing::warn!(target: "truapi::probe", "a warning of our own");
	});

	assert!(
		!lines.iter().any(|line| line.contains("a dependency warning")),
		"a dependency's warning reached the host sink: {lines:?}",
	);
	assert!(
		lines.iter().any(|line| line.contains("a warning of our own")),
		"our own warning did not reach the host sink: {lines:?}",
	);
}

#[test]
fn parse_level_defaults_to_the_error_floor() {
	assert_eq!(DEFAULT_LEVEL, LevelFilter::ERROR);
	assert_eq!(parse_level("nonsense"), DEFAULT_LEVEL);
	assert_eq!(parse_level(""), DEFAULT_LEVEL);
}

/// Silencing the channel stays possible, but only on purpose.
#[test]
fn parse_level_silences_only_on_an_explicit_request() {
	assert_eq!(parse_level("off"), LevelFilter::OFF);
	assert_eq!(parse_level("OFF"), LevelFilter::OFF);
	assert_eq!(parse_level("none"), LevelFilter::OFF);
}

/// The console stays at `ERROR` by default, so warnings reach a native
/// host only through the sink, and routine chatter must not.
#[test]
fn the_host_sink_receives_warnings_and_errors_only() {
	let lines = testing::host_sink_lines(|| {
		tracing::info!(target: "truapi::chain", "routine");
		tracing::warn!(target: "truapi::chain", kind = "Unavailable", "chain call failed");
		tracing::error!(target: "truapi::chain", "undecodable frame");
	});
	assert_eq!(
		lines,
		vec![
			"truapi.tracing.warn truapi::chain: chain call failed {kind=\"Unavailable\"}",
			"truapi.tracing.error truapi::chain: undecodable frame",
		]
	);
}

/// A runtime that is torn down must stop forwarding to its host, but must
/// not silence the runtime that replaced it.
#[test]
fn dropping_a_registration_removes_only_its_own_sink() {
	static SINK: RwLock<Option<HostSink>> = RwLock::new(None);
	let lines = Arc::new(std::sync::Mutex::new(Vec::new()));
	let recording = |name: &'static str| -> HostSink {
		let recorded = lines.clone();
		Arc::new(move |_marker, _detail| recorded.lock().unwrap().push(name))
	};
	let warn_through_sink = || {
		let subscriber =
			Registry::default().with(HostSinkLayer { sink: &SINK }.with_filter(HOST_SINK_LEVEL));
		tracing::subscriber::with_default(subscriber, || tracing::warn!("chain call failed"));
	};

	let first = HostSinkRegistration::install(&SINK, recording("first"));
	let second = HostSinkRegistration::install(&SINK, recording("second"));
	drop(first);
	warn_through_sink();
	drop(second);
	warn_through_sink();

	// Dropping the one that does not own the slot changes nothing, and
	// dropping the owner puts back what it displaced. Restoring is what
	// lets a host install before the rest of its startup: a runtime that
	// fails to start leaves the one already running still logging.
	//
	// The second line here is the cost of that: `first` was already
	// dropped, so its sink is put back with nobody behind it. A native
	// host's sink holds its callbacks weakly and goes quiet when its
	// runtime does, so a restored sink with no runtime says nothing.
	assert_eq!(*lines.lock().unwrap(), vec!["second", "first"]);
}

/// A host exception message is unbounded and leaves the device, so the
/// sink caps it without splitting a character.
#[test]
fn the_host_sink_bounds_the_detail_in_characters() {
	let reason = "\u{e9}".repeat(10_000);
	let lines = testing::host_sink_lines(|| {
		tracing::warn!(target: "truapi::chain", "{reason}");
	});

	let detail: String = format!("truapi::chain: {reason}")
		.chars()
		.take(HOST_SINK_DETAIL_MAX_CHARS)
		.collect();
	assert_eq!(lines, vec![format!("truapi.tracing.warn {detail}")]);
}

#[test]
fn parse_level_reads_each_named_level() {
	assert_eq!(parse_level("error"), LevelFilter::ERROR);
	assert_eq!(parse_level("warn"), LevelFilter::WARN);
	assert_eq!(parse_level("warning"), LevelFilter::WARN);
	assert_eq!(parse_level("info"), LevelFilter::INFO);
	assert_eq!(parse_level("debug"), LevelFilter::DEBUG);
	assert_eq!(parse_level("trace"), LevelFilter::TRACE);
}
