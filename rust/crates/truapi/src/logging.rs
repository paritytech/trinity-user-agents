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
//! [`markers::HOST_MARKER_PREFIX`] is addressed to the host rather than to the
//! console, and its target is the marker the host receives. Any warning or
//! error is forwarded too, under a marker naming its level, so a host's log
//! does not depend on anyone lowering the level for everything else.
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

use crate::markers;
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

/// How much of an account a line that leaves the device may name.
const ACCOUNT_HANDLE_BYTES: usize = 4;

/// An account named in a log line.
///
/// Both `Display` and `Debug` write a short handle of the account's leading
/// bytes, so either sigil in a `tracing` field is safe to use. The handle is
/// enough to tell two accounts apart within one log and is not the identifier
/// itself. A host ships warnings and errors to a crash reporter, where a whole
/// account id is a public key that indexes everything that account has ever
/// done.
///
/// A line that stays on the device names the account in full through
/// [`Account::in_full`], which has to be asked for by name.
#[derive(Clone, Copy)]
pub struct Account<'a>(&'a [u8]);

impl<'a> Account<'a> {
	/// Names `account` in a log line.
	pub fn new(account: &'a [u8]) -> Self {
		Self(account)
	}

	/// The whole account, for a line that stays on the device.
	pub fn in_full(&self) -> String {
		hex::encode(self.0)
	}
}

impl fmt::Display for Account<'_> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		for byte in &self.0[..self.0.len().min(ACCOUNT_HANDLE_BYTES)] {
			write!(f, "{byte:02x}")?;
		}
		Ok(())
	}
}

impl fmt::Debug for Account<'_> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Display::fmt(self, f)
	}
}

/// Longest a host-supplied string may run in a line that leaves the device,
/// counted after escaping and not counting the quotes around it.
const HOST_TEXT_MAX_CHARS: usize = 160;

/// Text a host handed the core, named in a log line.
///
/// A foreign exception message is written by code the core does not control.
/// It can run to any length, can carry newlines and the separators a detail
/// is built from, and the host ships warnings and errors to a crash reporter.
/// Both `Display` and `Debug` write it quoted and escaped the way a Rust
/// string literal is written, and stop after a bounded number of characters,
/// so it stays on one line, cannot be read as the fields beside it, and
/// cannot reorder what follows it.
///
/// Escaping goes through `char::escape_debug`, which covers the categories
/// `char::is_control` leaves out, the line and paragraph separators and the
/// bidirectional overrides among them.
///
/// The wording of the text is not otherwise rewritten. This bounds its shape,
/// and it is the one place a rule about its content would go.
#[derive(Clone, Copy)]
pub struct HostText<'a>(&'a str);

impl<'a> HostText<'a> {
	/// Names `text`, which came from a host, in a log line.
	pub fn new(text: &'a str) -> Self {
		Self(text)
	}
}

impl fmt::Display for HostText<'_> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_char('"')?;
		let stopped_early = write_bounded_escaped(f, self.0)?;
		f.write_char('"')?;
		if stopped_early {
			f.write_char('\u{2026}')?;
		}
		Ok(())
	}
}

/// Writes `text` escaped into `out`, stopping at [`HOST_TEXT_MAX_CHARS`].
///
/// Returns whether it stopped early. The bound counts the escape rather than
/// the character behind it, so what is written is what is bounded and an
/// escape is never cut in half.
fn write_bounded_escaped(out: &mut impl fmt::Write, text: &str) -> Result<bool, fmt::Error> {
	let mut written = 0;
	for character in text.chars() {
		let escaped = character.escape_debug();
		if written + escaped.len() > HOST_TEXT_MAX_CHARS {
			return Ok(true);
		}
		written += escaped.len();
		for part in escaped {
			out.write_char(part)?;
		}
	}
	Ok(false)
}

/// `text`, as a host handed it, bounded and escaped so it is safe to keep.
///
/// [`HostText`] makes a host's string safe in the one line that names it.
/// A string the core stores and hands on travels to places that line cannot
/// reach: the warnings raised about it later, and the product the error is
/// returned to. Bounding it where it enters means the value carries its own
/// bound, so none of those places has to remember.
///
/// The result is one line, bounded to the same number of characters as the
/// log field, plus a mark when something was dropped. It is not quoted,
/// because it is read by a person rather than by a log parser.
pub fn bound_host_text(text: &str) -> String {
	let mut bounded = String::new();
	if write_bounded_escaped(&mut bounded, text).expect("writing to a String cannot fail") {
		bounded.push('\u{2026}');
	}
	bounded
}

impl fmt::Debug for HostText<'_> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Display::fmt(self, f)
	}
}

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
				if !meta.target().starts_with(CRATE_TARGET_PREFIX) {
					return false;
				}
				if meta.target().starts_with(markers::HOST_MARKER_PREFIX) {
					// A host-addressed target the table does not list is a
					// mistake, not a line to pass on: nothing downstream knows
					// what its detail carries.
					return markers::class_of(meta.target()).is_some();
				}
				// A module path reaches a host only for being a warning, and
				// arrives under the `truapi.tracing.*` markers the layer builds.
				*meta.level() <= HOST_SINK_EVENT_LEVEL
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

/// Keeps a host sink installed until dropped, and puts back whatever it
/// displaced.
///
/// Restoring rather than clearing is what lets a host install its sink before
/// the rest of its startup, which may fail: a runtime that does not finish
/// starting leaves the one already running still logging.
#[must_use = "dropping the registration puts back the sink it displaced"]
pub struct HostSinkRegistration {
	slot: &'static RwLock<Option<HostSink>>,
	sink: HostSink,
	displaced: Option<HostSink>,
}

impl HostSinkRegistration {
	fn install(slot: &'static RwLock<Option<HostSink>>, sink: HostSink) -> Self {
		// `replace` hands the previous sink back rather than dropping it here,
		// so no host code runs under the write lock.
		let displaced = slot
			.write()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
			.replace(sink.clone());
		Self { slot, sink, displaced }
	}
}

impl Drop for HostSinkRegistration {
	fn drop(&mut self) {
		let displaced = self.displaced.take();
		let dropped = {
			let mut installed = self.slot.write().unwrap_or_else(|poisoned| poisoned.into_inner());
			if installed.as_ref().is_some_and(|installed| Arc::ptr_eq(installed, &self.sink)) {
				core::mem::replace(&mut *installed, displaced)
			} else {
				// A later registration owns the slot, so it owns what it
				// displaced too, and this one has nothing to put back.
				displaced
			}
		};
		drop(dropped);
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
		let (marker, detail) = if meta.target().starts_with(markers::HOST_MARKER_PREFIX) {
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

/// The first `max_chars` characters of `text`, as a slice.
///
/// Bounds what a detail may carry across the boundary, on the tracing path and
/// on the bridge path alike.
pub(crate) fn truncate_to_chars(text: &str, max_chars: usize) -> &str {
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
mod tests;
