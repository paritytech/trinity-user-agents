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
//! An event whose target starts with [`HOST_MARKER_PREFIX`] is addressed to the
//! host rather than to the console: it reaches the sink a host installs with
//! [`set_host_log_sink`] whatever the level is, so the few conditions a host
//! operator has to see do not depend on the level being lowered for everything
//! else.
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
	Arc, Mutex, OnceLock,
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

/// Prefix that marks an event's target as addressed to the host rather than
/// naming a module. Rust module paths separate with `::`, so a dotted target
/// cannot collide with one, and the rest of the target names the condition.
pub const HOST_MARKER_PREFIX: &str = "truapi.";

/// Loudest level a marked event may use.
///
/// A filter with no level opinion pins the process-wide max level at `TRACE`,
/// which costs every `debug!` in every crate the static check that would
/// otherwise cut it before any dispatch. Raise this before marking anything
/// quieter than it.
const HOST_MARKER_MAX_LEVEL: LevelFilter = LevelFilter::INFO;

/// What a host installs to read the events addressed to it: the event's target
/// as a marker slug and its rendered message as the detail, called from
/// whatever thread raised the event.
pub type HostLogSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Host sink for marked events, when a host has one.
///
/// A `Mutex` rather than a `OnceLock` so a test can install one and take it
/// away again. Marked events are rare, so the lock is never hot.
static HOST_SINK: Mutex<Option<HostLogSink>> = Mutex::new(None);

/// Send every event whose target starts with [`HOST_MARKER_PREFIX`] to `sink`,
/// replacing any previous one. Hosts that surface a lifecycle log call this
/// once; the rest leave it unset, and those events then reach only the console,
/// subject to the current level.
pub fn set_host_log_sink(sink: HostLogSink) {
	// Released outside the lock: dropping the previous sink runs host code,
	// which must not be able to re-enter a lock this thread already holds.
	let previous = HOST_SINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).replace(sink);
	drop(previous);
}

fn host_log_sink() -> Option<HostLogSink> {
	HOST_SINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

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

/// Install the global subscriber. Idempotent: the first call wins, later
/// calls (and a foreign subscriber already being set) are no-ops.
pub fn init() {
	if RELOAD_HANDLE.get().is_some() {
		return;
	}
	let (filter, handle) = reload::Layer::<LevelFilter, Registry>::new(DEFAULT_LEVEL);
	let subscriber = Registry::default().with(ConsoleLayer.with_filter(filter)).with(
		HostSinkLayer.with_filter(
			filter_fn(|meta: &tracing::Metadata<'_>| meta.target().starts_with(HOST_MARKER_PREFIX))
				.with_max_level_hint(HOST_MARKER_MAX_LEVEL),
		),
	);
	if tracing::subscriber::set_global_default(subscriber).is_ok() {
		let _ = RELOAD_HANDLE.set(handle);
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
struct HostSinkLayer;

impl<S> Layer<S> for HostSinkLayer
where
	S: Subscriber,
	S: for<'a> LookupSpan<'a>,
{
	fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
		if IN_HOST_SINK.with(Cell::get) {
			return;
		}
		let Some(sink) = host_log_sink() else {
			return;
		};
		let mut visitor = EventVisitor::default();
		event.record(&mut visitor);
		let mut detail = visitor.message;
		if !visitor.fields.is_empty() {
			let _ = write!(detail, " {{{}}}", visitor.fields);
		}
		IN_HOST_SINK.with(|inside| inside.set(true));
		let _reentry = HostSinkReentry;
		sink(event.metadata().target(), &detail);
	}
}

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

		let mut line = format!("[truapi] {} {}", meta.level(), meta.target());
		if !visitor.message.is_empty() {
			let _ = write!(line, ": {}", visitor.message);
		}
		if !visitor.fields.is_empty() {
			let _ = write!(line, " {{{}}}", visitor.fields);
		}
		emit(*meta.level(), &line);
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

#[cfg(test)]
mod tests {
	use super::*;

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
		set_host_log_sink(Arc::new(move |marker: &str, detail: &str| {
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
		*HOST_SINK.lock().unwrap() = None;
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
		set_host_log_sink(Arc::new(move |_marker: &str, _detail: &str| {
			*recorder.lock().unwrap() += 1;
			tracing::info!(target: "truapi.test.echo", "raised from inside the sink");
		}));

		tracing::info!(target: "truapi.test.reentrancy", "one marked event");

		assert_eq!(*depth.lock().unwrap(), 1, "the event the sink raised re-entered the sink",);

		// Leave no sink behind for other tests in this binary.
		*HOST_SINK.lock().unwrap() = None;
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

#[cfg(test)]
mod maxlevel_probe {
	use super::*;

	#[test]
	fn probe_global_max_level() {
		init();
		set_level(DEFAULT_LEVEL);
		println!("PROBE at ERROR: current={:?}", LevelFilter::current());
		set_level(LevelFilter::OFF);
		println!("PROBE at OFF:   current={:?}", LevelFilter::current());
		println!("PROBE debug enabled? {}", tracing::enabled!(tracing::Level::DEBUG));
	}
}
