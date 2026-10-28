//! Level-controlled `tracing` output, routed to the host console.
//!
//! Events emitted via the `tracing` macros (`info!`, `debug!`, …) and
//! `#[instrument]` spans flow through a single subscriber installed once by
//! [`init`]. A reloadable [`LevelFilter`] decides what reaches the console, so
//! the verbosity is tunable at runtime via [`set_level`] (exposed to JS as
//! `setLogLevel`). The default floor is [`DEFAULT_LEVEL`]: the crate reserves
//! `ERROR` for protocol violations a peer cannot see any other way (an
//! undecodable frame, an unroutable discriminant pair), so a host that never
//! calls `setLogLevel` still learns that its peer is speaking a wire it does
//! not understand. Everything chattier stays off until asked for, and
//! `setLogLevel("off")` silences the console completely.
//!
//! Two kinds of event reach the sink a host installs with [`set_host_sink`],
//! whatever the console level is, because a native host keeps stderr out of its
//! log files and crash reports. An event whose target starts with
//! [`HOST_MARKER_PREFIX`] is addressed to the host rather than to the console,
//! and its target is the marker the host receives. Any warning or error is
//! forwarded too, under a marker naming its level, so a host's log does not
//! depend on anyone lowering the level for everything else.
//!
//! On wasm each level maps to the matching `console` method
//! (`error`/`warn`/`info`/`debug`); on native everything goes to stderr.
//! In Chrome, `debug`/`trace` land on `console.debug`, which the DevTools
//! console hides unless its level dropdown includes "Verbose".
//!
//! Output is plaintext, so never log secret material (key bytes, session
//! tokens, signatures).

use core::{
	cell::Cell,
	fmt::{self, Write as _},
};
use std::sync::{
	Arc, OnceLock, RwLock,
	atomic::{AtomicBool, Ordering},
};

use tracing::{
	Event, Id, Level, Subscriber,
	field::{Field, Visit},
	span::{Attributes, Record},
};
use tracing_subscriber::{
	Registry,
	filter::{LevelFilter, filter_fn},
	layer::{Context, Layer, SubscriberExt as _},
	registry::LookupSpan,
	reload,
};

static RELOAD_HANDLE: OnceLock<reload::Handle<LevelFilter, Registry>> = OnceLock::new();
static TRACE_SPANS: AtomicBool = AtomicBool::new(false);
static HOST_SINK: RwLock<Option<HostSink>> = RwLock::new(None);

/// Takes a marker and a detail line for the native host log.
type HostSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Prefix that marks an event's target as addressed to the host rather than
/// naming a module. Rust module paths separate with `::`, so a dotted target
/// cannot collide with one, and the rest of the target names the condition.
pub const HOST_MARKER_PREFIX: &str = "truapi.";

/// Targets this crate family owns, whether a module path (`truapi::chain_runtime`)
/// or a marker addressed to the host (`truapi.chain.follow_budget_reached`). A
/// dependency's own `tracing` traffic is not a host's business and must not be
/// able to drive the host sink.
const CRATE_TARGET_PREFIX: &str = "truapi";

/// Loudest level a marked event may use.
///
/// A filter with no level opinion pins the process-wide max level at `TRACE`,
/// which costs every `debug!` in every crate the static check that would
/// otherwise cut it before any dispatch. Raise this before marking anything
/// quieter than it.
const HOST_MARKER_MAX_LEVEL: LevelFilter = LevelFilter::INFO;

thread_local! {
	/// Set while this thread is inside the host's sink. A globally installed
	/// subscriber takes `tracing`'s fast path, which carries no reentrancy
	/// guard, so an event the sink raises itself would re-enter it until the
	/// stack is gone.
	static IN_HOST_SINK: Cell<bool> = const { Cell::new(false) };
}

/// Clears [`IN_HOST_SINK`] however the sink call ends, a panic included.
struct HostSinkReentry;

impl Drop for HostSinkReentry {
	fn drop(&mut self) {
		IN_HOST_SINK.with(|inside| inside.set(false));
	}
}

/// Verbosity applied until a host calls [`set_level`], and the fallback for an
/// unrecognised level string. `ERROR` is reserved for protocol violations, so
/// this floor keeps those visible without emitting routine traffic.
pub const DEFAULT_LEVEL: LevelFilter = LevelFilter::ERROR;

/// Lowest level that reaches the host sink, independent of [`set_level`].
pub const HOST_SINK_LEVEL: LevelFilter = LevelFilter::WARN;

/// [`HOST_SINK_LEVEL`] as an event level, for comparing one event against it.
const HOST_SINK_EVENT_LEVEL: Level = Level::WARN;

/// Longest detail the host sink forwards, in characters.
///
/// A warning can carry a host exception message, which is unbounded and can
/// embed paths or arguments, and the host ships these lines off the device.
pub const HOST_SINK_DETAIL_MAX_CHARS: usize = 512;

/// Install the global subscriber. Idempotent: the first call wins, later
/// calls (and a foreign subscriber already being set) are no-ops.
pub fn init() {
	if RELOAD_HANDLE.get().is_some() {
		return;
	}
	let (filter, handle) = reload::Layer::<LevelFilter, Registry>::new(DEFAULT_LEVEL);
	if tracing::subscriber::set_global_default(subscriber(filter, &HOST_SINK)).is_ok() {
		let _ = RELOAD_HANDLE.set(handle);
	}
}

fn subscriber(
	filter: reload::Layer<LevelFilter, Registry>,
	slot: &'static RwLock<Option<HostSink>>,
) -> impl Subscriber {
	Registry::default().with(ConsoleLayer.with_filter(filter)).with(
		HostSinkLayer { sink: slot }.with_filter(
			filter_fn(|meta: &tracing::Metadata<'_>| {
				// Only this crate's own events. The reentrancy guard is
				// thread-local, so it does not catch a host that hands a line to
				// another thread which logs; admitting every crate's warnings
				// would make that loop reachable through any dependency, and
				// would ship their lines off the device besides.
				meta.target().starts_with(CRATE_TARGET_PREFIX) &&
					(meta.target().starts_with(HOST_MARKER_PREFIX) ||
						*meta.level() <= HOST_SINK_EVENT_LEVEL)
			})
			.with_max_level_hint(HOST_MARKER_MAX_LEVEL),
		),
	)
}

/// Send each event at [`HOST_SINK_LEVEL`] or above to `sink` as a marker
/// (`truapi.tracing.warn` or `truapi.tracing.error`) and a detail line.
///
/// One sink per process: the latest replaces any earlier one, and dropping the
/// returned registration removes the sink unless a later one replaced it.
/// Between a registration dropping and the next one installing, no sink is
/// set, so build one runtime per process.
pub fn set_host_sink(sink: HostSink) -> HostSinkRegistration {
	HostSinkRegistration::install(&HOST_SINK, sink)
}

/// Whether any host still has a sink installed.
#[cfg(test)]
pub(crate) fn host_sink_installed() -> bool {
	HOST_SINK.read().unwrap_or_else(|poisoned| poisoned.into_inner()).is_some()
}

/// Keeps a host sink installed until dropped.
#[must_use = "dropping the registration removes the sink"]
pub struct HostSinkRegistration {
	slot: &'static RwLock<Option<HostSink>>,
	sink: HostSink,
}

impl HostSinkRegistration {
	fn install(slot: &'static RwLock<Option<HostSink>>, sink: HostSink) -> Self {
		*slot.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sink.clone());
		Self { slot, sink }
	}
}

impl Drop for HostSinkRegistration {
	fn drop(&mut self) {
		let mut installed = self.slot.write().unwrap_or_else(|poisoned| poisoned.into_inner());
		if installed.as_ref().is_some_and(|installed| Arc::ptr_eq(installed, &self.sink)) {
			*installed = None;
		}
	}
}

/// Set the live verbosity threshold. No-op until [`init`] has run.
pub fn set_level(level: LevelFilter) {
	TRACE_SPANS.store(level == LevelFilter::TRACE, Ordering::Relaxed);
	if let Some(handle) = RELOAD_HANDLE.get() {
		let _ = handle.reload(level);
	}
}

/// Apply a host-supplied level string, installing the subscriber first so the
/// call works regardless of whether the core has been constructed yet, then
/// emitting a confirmation event so hosts can verify the logging pipeline end
/// to end. The confirmation is logged at `INFO` (mapping to `console.info`,
/// visible without DevTools "Verbose") rather than at the level just set, so it
/// surfaces even when `debug`/`trace` events land on the hidden `console.debug`.
pub fn set_level_from_str(level: &str) {
	init();
	set_level(parse_level(level));
	tracing::info!(level, "log level set");
}

/// Parse a host-supplied level string. Only an explicit `"off"` silences the
/// channel; an unrecognised value falls back to [`DEFAULT_LEVEL`] so a typo
/// cannot quietly suppress protocol violations.
pub fn parse_level(level: &str) -> LevelFilter {
	match level.to_ascii_lowercase().as_str() {
		"off" | "none" => LevelFilter::OFF,
		"error" => LevelFilter::ERROR,
		"warn" | "warning" => LevelFilter::WARN,
		"info" => LevelFilter::INFO,
		"debug" => LevelFilter::DEBUG,
		"trace" => LevelFilter::TRACE,
		_ => DEFAULT_LEVEL,
	}
}

/// Forwards events addressed to the host to its sink.
///
/// The reloadable level filter governs the console layer alone, so an event a
/// host operator needs arrives at the default `ERROR` floor without turning on
/// the rest of the crate's `warn!` traffic.
/// Routes each event to the console method matching its level.
struct ConsoleLayer;

impl<S> Layer<S> for ConsoleLayer
where
	S: Subscriber,
	S: for<'a> LookupSpan<'a>,
{
	fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
		let Some(span) = ctx.span(id) else {
			return;
		};
		let mut visitor = EventVisitor::default();
		attrs.record(&mut visitor);
		span.extensions_mut().insert(SpanFields { fields: visitor.fields });
		if trace_spans_enabled() {
			emit_span("new", &span);
		}
	}

	fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
		let Some(span) = ctx.span(id) else {
			return;
		};
		let mut visitor = EventVisitor::default();
		values.record(&mut visitor);
		if visitor.fields.is_empty() {
			return;
		}
		let mut extensions = span.extensions_mut();
		if let Some(fields) = extensions.get_mut::<SpanFields>() {
			if !fields.fields.is_empty() {
				fields.fields.push_str(", ");
			}
			fields.fields.push_str(&visitor.fields);
		} else {
			extensions.insert(SpanFields { fields: visitor.fields });
		}
	}

	fn on_close(&self, id: Id, ctx: Context<'_, S>) {
		if !trace_spans_enabled() {
			return;
		}
		let Some(span) = ctx.span(&id) else {
			return;
		};
		emit_span("close", &span);
	}

	fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
		let meta = event.metadata();
		let mut visitor = EventVisitor::default();
		event.record(&mut visitor);

		let line = format!("[truapi] {} {}", meta.level(), visitor.describe(meta.target()));
		emit(*meta.level(), &line);
	}
}

/// Hands each event to the sink installed by [`set_host_sink`].
struct HostSinkLayer {
	sink: &'static RwLock<Option<HostSink>>,
}

impl<S: Subscriber> Layer<S> for HostSinkLayer {
	fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
		if IN_HOST_SINK.with(Cell::get) {
			return;
		}
		// Cloned out so a sink that logs again cannot deadlock on the lock.
		let sink = self.sink.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
		let Some(sink) = sink else {
			return;
		};
		let meta = event.metadata();
		let mut visitor = EventVisitor::default();
		event.record(&mut visitor);
		// A marked target names its own condition, so it is the marker. Anything
		// else reaches a host only for being a warning, so the marker names the
		// level and the detail keeps the module that raised it.
		let (marker, detail) = if meta.target().starts_with(HOST_MARKER_PREFIX) {
			(meta.target().to_string(), visitor.detail())
		} else {
			(
				format!("truapi.tracing.{}", meta.level().as_str().to_ascii_lowercase()),
				visitor.describe(meta.target()),
			)
		};
		IN_HOST_SINK.with(|inside| inside.set(true));
		let _reentry = HostSinkReentry;
		sink(&marker, truncate_to_chars(&detail, HOST_SINK_DETAIL_MAX_CHARS));
	}
}

fn truncate_to_chars(text: &str, max_chars: usize) -> &str {
	match text.char_indices().nth(max_chars) {
		Some((end, _)) => &text[..end],
		None => text,
	}
}

#[derive(Default)]
struct SpanFields {
	fields: String,
}

fn trace_spans_enabled() -> bool {
	TRACE_SPANS.load(Ordering::Relaxed)
}

fn emit_span<S>(kind: &str, span: &tracing_subscriber::registry::SpanRef<'_, S>)
where
	S: Subscriber,
	S: for<'a> LookupSpan<'a>,
{
	let meta = span.metadata();
	let mut line = format!("[truapi] TRACE {}: span {}", meta.target(), kind);
	let extensions = span.extensions();
	let fields = extensions.get::<SpanFields>();
	let _ = write!(line, " {{span={:?}", meta.name());
	if let Some(fields) = fields &&
		!fields.fields.is_empty()
	{
		let _ = write!(line, ", {}", fields.fields);
	}
	line.push('}');
	emit(Level::TRACE, &line);
}

/// Collects the implicit `message` field separately from explicit key-values.
#[derive(Default)]
struct EventVisitor {
	message: String,
	fields: String,
}

impl EventVisitor {
	/// The line for a marked event, whose target the sink already carries as
	/// the marker, so repeating it here would only say it twice.
	fn detail(&self) -> String {
		let mut line = self.message.clone();
		if !self.fields.is_empty() {
			let _ = write!(line, " {{{}}}", self.fields);
		}
		line
	}

	fn describe(&self, target: &str) -> String {
		let mut line = target.to_string();
		if !self.message.is_empty() {
			let _ = write!(line, ": {}", self.message);
		}
		if !self.fields.is_empty() {
			let _ = write!(line, " {{{}}}", self.fields);
		}
		line
	}
}

impl Visit for EventVisitor {
	fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
		if field.name() == "message" {
			let _ = write!(self.message, "{value:?}");
		} else {
			if !self.fields.is_empty() {
				self.fields.push_str(", ");
			}
			let _ = write!(self.fields, "{}={value:?}", field.name());
		}
	}
}

#[cfg(not(target_arch = "wasm32"))]
fn emit(_level: Level, line: &str) {
	eprintln!("{line}");
}

#[cfg(target_arch = "wasm32")]
fn emit(level: Level, line: &str) {
	let js = wasm_bindgen::JsValue::from_str(line);
	match level {
		Level::ERROR => web_sys::console::error_1(&js),
		Level::WARN => web_sys::console::warn_1(&js),
		Level::INFO => web_sys::console::info_1(&js),
		Level::DEBUG | Level::TRACE => web_sys::console::debug_1(&js),
	}
}

/// Test helpers for code whose logging reaches the host sink.
#[cfg(test)]
pub mod testing {
	use super::*;

	/// Runs `emit` with the host sink layer on this thread and returns each line
	/// it forwards as `marker detail`.
	pub fn host_sink_lines(emit: impl FnOnce()) -> Vec<String> {
		// A global default stops tracing-core from caching a callsite as never
		// enabled when a thread with no subscriber registers it first.
		init();
		let lines = Arc::new(std::sync::Mutex::new(Vec::new()));
		let recorded = lines.clone();
		let sink: HostSink = Arc::new(move |marker, detail| {
			recorded.lock().unwrap().push(format!("{marker} {detail}"));
		});
		// Leaked so each call gets its own slot and parallel tests cannot share a sink.
		let slot: &'static RwLock<Option<HostSink>> = Box::leak(Box::new(RwLock::new(Some(sink))));
		tracing::subscriber::with_default(
			subscriber(reload::Layer::new(LevelFilter::OFF).0, slot),
			emit,
		);
		core::mem::take(&mut *lines.lock().unwrap())
	}

	/// Calls the sink `registration` holds, wherever the process-wide slot points now.
	pub fn forward(registration: &HostSinkRegistration, marker: &str, detail: &str) {
		(registration.sink)(marker, detail);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Mutex;

	/// The host sink is process-wide, so a test that installs one has to own it
	/// for the whole of its run or a sibling's teardown reads as its own.
	static SINK_TESTS: Mutex<()> = Mutex::new(());

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

		tracing::warn!(target: "truapi.test.marker", "something a host operator needs to see");
		tracing::warn!(target: "truapi::test", "routine crate traffic");

		// Filtered rather than compared whole: other tests in this binary share
		// the subscriber and can raise their own marked events.
		let seen = seen.lock().unwrap().clone();
		assert!(
			seen.contains(&(
				"truapi.test.marker".to_string(),
				"something a host operator needs to see".to_string()
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
			tracing::info!(target: "truapi.test.echo", "and a marked one");
		}));

		tracing::info!(target: "truapi.test.reentrancy", "one marked event");

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
			let subscriber = Registry::default()
				.with(HostSinkLayer { sink: &SINK }.with_filter(HOST_SINK_LEVEL));
			tracing::subscriber::with_default(subscriber, || tracing::warn!("chain call failed"));
		};

		let first = HostSinkRegistration::install(&SINK, recording("first"));
		let second = HostSinkRegistration::install(&SINK, recording("second"));
		drop(first);
		warn_through_sink();
		drop(second);
		warn_through_sink();

		assert_eq!(*lines.lock().unwrap(), vec!["second"]);
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
}
