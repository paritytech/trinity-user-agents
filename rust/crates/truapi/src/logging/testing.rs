use super::*;

/// Runs `emit` with the host sink layer on this thread and returns each line
/// it forwards as `marker detail`.
pub fn host_sink_lines(emit: impl FnOnce()) -> Vec<String> {
	// A global default stops tracing-core from caching a callsite as never
	// enabled when a thread with no subscriber registers it first.
	init();
	let lines = Arc::new(std::sync::Mutex::new(Vec::new()));
	let recorded = lines.clone();
	let sink: HostSinkHandle = Arc::new(move |marker: &str, detail: &str| {
		recorded.lock().unwrap().push(format!("{marker} {detail}"));
	});
	// Leaked so each call gets its own slot and parallel tests cannot share a sink.
	let slot: &'static RwLock<Option<HostSinkHandle>> =
		Box::leak(Box::new(RwLock::new(Some(sink))));
	tracing::subscriber::with_default(
		subscriber(reload::Layer::new(LevelFilter::OFF).0, slot),
		emit,
	);
	core::mem::take(&mut *lines.lock().unwrap())
}

/// Calls the sink `registration` holds, wherever the process-wide slot points now.
pub fn forward(registration: &HostSinkRegistration, marker: &str, detail: &str) {
	registration.sink.log(marker, detail);
}
