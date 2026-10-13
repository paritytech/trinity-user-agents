//! Native `log` sink for the embedded light client.
//!
//! smoldot emits through `log::logger()` (see `smoldot_light::platform::default`),
//! and a Rust staticlib carries its own copy of the global logger the `log` crate
//! state. Every mobile shell links the provider as a separate staticlib, so it
//! cannot make smoldot output visible by installing a bridge on its own side.
//! The logger has to be set inside this library. Without
//! it the sync, peer and networking detail from smoldot is discarded and a light client
//! that never produces blocks looks identical to one that is merely slow.
//!
//! Off unless `TRUAPI_PROVIDER_LOG` names a level, so a shipping app pays
//! nothing. Plaintext to stderr: never log secret material.

use std::{io::Write as _, sync::Once};

static INSTALL: Once = Once::new();

struct StderrLogger;

impl log::Log for StderrLogger {
	fn enabled(&self, metadata: &log::Metadata) -> bool {
		metadata.level() <= log::max_level()
	}

	fn log(&self, record: &log::Record) {
		if !self.enabled(record.metadata()) {
			return;
		}
		// One write per record. Interleaved fragments from the worker
		// threads would be unreadable.
		let line =
			format!("[provider] {:<5} {}: {}\n", record.level(), record.target(), record.args());
		let _ = std::io::stderr().write_all(line.as_bytes());
	}

	fn flush(&self) {
		let _ = std::io::stderr().flush();
	}
}

/// Installs the stderr logger from `TRUAPI_PROVIDER_LOG`, once. Unset or
/// unrecognised leaves logging off.
pub fn init_from_env() {
	INSTALL.call_once(|| {
		let level = match std::env::var("TRUAPI_PROVIDER_LOG")
			.unwrap_or_default()
			.trim()
			.to_ascii_lowercase()
			.as_str()
		{
			"trace" => log::LevelFilter::Trace,
			"debug" => log::LevelFilter::Debug,
			"info" => log::LevelFilter::Info,
			"warn" => log::LevelFilter::Warn,
			"error" => log::LevelFilter::Error,
			_ => log::LevelFilter::Off,
		};
		if level == log::LevelFilter::Off {
			return;
		}
		if log::set_boxed_logger(Box::new(StderrLogger)).is_ok() {
			log::set_max_level(level);
			let _ = std::io::stderr()
				.write_all(format!("[provider] logging installed at {level}\n").as_bytes());
		}
	});
}
