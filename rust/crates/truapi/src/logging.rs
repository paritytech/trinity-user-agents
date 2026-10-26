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
//! `setLogLevel("off")` silences the channel completely.
//!
//! On wasm each level maps to the matching `console` method
//! (`error`/`warn`/`info`/`debug`); on native everything goes to stderr.
//! In Chrome, `debug`/`trace` land on `console.debug`, which the DevTools
//! console hides unless its level dropdown includes "Verbose".
//!
//! Output is plaintext, so never log secret material (key bytes, session
//! tokens, signatures).

use core::fmt::{self, Write as _};
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
	filter::LevelFilter,
	layer::{Context, Layer, SubscriberExt as _},
	registry::LookupSpan,
	reload,
};

static RELOAD_HANDLE: OnceLock<reload::Handle<LevelFilter, Registry>> = OnceLock::new();
static TRACE_SPANS: AtomicBool = AtomicBool::new(false);

/// What a host installs to receive core notices: a marker slug and a
/// free-form detail, called from whatever thread raised the notice.
pub type HostNoticeSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Host sink for core notices, when a host has one.
///
/// A `Mutex` rather than a `OnceLock` so a test can install one and take it
/// away again; notices are rare, so the lock is never hot.
static HOST_NOTICES: Mutex<Option<HostNoticeSink>> = Mutex::new(None);

/// Send core notices to `sink`, replacing any previous one. Hosts that surface
/// a lifecycle log call this once; the rest leave it unset and notices only
/// reach tracing.
pub fn set_host_notice_sink(sink: HostNoticeSink) {
	*HOST_NOTICES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sink);
}

/// Report something a host operator needs to see regardless of trace level.
///
/// The level filter exists to keep routine traffic off a host's log, and it
/// defaults to `ERROR`, so a `warn!` never reaches a device. A notice is for
/// the few conditions that are worth a host log entry on their own: it goes to
/// the host's sink directly, and to tracing as well for hosts that read that
/// instead. `marker` is a stable slug a log can be filtered by; `detail` is
/// free-form.
pub fn notice(marker: &str, detail: &str) {
	let sink = HOST_NOTICES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
	if let Some(sink) = sink {
		sink(marker, detail);
	}
	tracing::warn!(target: "truapi::notice", marker, detail, "core notice");
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
	let subscriber = Registry::default().with(ConsoleLayer.with_filter(filter));
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

	/// The floor must stay at `ERROR` so a host that never calls
	/// `setLogLevel` still sees protocol violations, and a typo must not be
	/// able to silence them.
	/// A notice reaches the host's sink even though the level filter is at its
	/// default, which is what a `warn!` cannot do.
	#[test]
	fn a_notice_reaches_the_host_sink_at_the_default_level() {
		init();
		set_level(DEFAULT_LEVEL);
		let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
		let recorder = seen.clone();
		set_host_notice_sink(Arc::new(move |marker: &str, detail: &str| {
			recorder.lock().unwrap().push((marker.to_string(), detail.to_string()));
		}));

		notice("truapi.test.marker", "something a host operator needs to see");

		assert_eq!(
			seen.lock().unwrap().clone(),
			vec![(
				"truapi.test.marker".to_string(),
				"something a host operator needs to see".to_string()
			)],
			"the sink receives the notice with the default ERROR filter in place",
		);

		// Leave no sink behind for other tests in this binary.
		*HOST_NOTICES.lock().unwrap() = None;
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
