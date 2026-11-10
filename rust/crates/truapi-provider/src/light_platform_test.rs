//! Test-only [`PlatformRef`] wrapper that collapses smoldot's sync-mode
//! decision deadline.
//!
//! A light client with no reachable peers cannot decide between warp sync and
//! all-forks sync from network activity, so it holds every `SubscribeAll`
//! request queued until its 30-second `MODE_DECISION_TIMEOUT` fires and commits
//! it to all-forks. Offline that outcome is fixed from the start, and the wait
//! ahead of it is idle: the same state is reached either way, 30 seconds later.
//!
//! The deadline is armed as `platform.sleep(MODE_DECISION_TIMEOUT)`, and the
//! platform is ours, so shortening that one sleep is the only seam smoldot
//! offers. Every other literal `platform.sleep` duration in `smoldot-light` is
//! ten seconds or under, so [`LONG_SLEEP`] selects the mode-decision deadline
//! and leaves connection backoff and discovery alone.
//!
//! Modelled on `smoldot_light::platform::WithPrefix`, which wraps a platform
//! the same way to prefix its log targets.

use core::{pin::Pin, time::Duration};
use std::borrow::Cow;

use smoldot_light::platform::{Address, ConnectionType, LogLevel, MultiStreamAddress, PlatformRef};

/// Sleeps at least this long are collapsed to [`COLLAPSED_SLEEP`]. Sits above
/// smoldot's other sleeps and below its 30-second mode-decision deadline.
const LONG_SLEEP: Duration = Duration::from_secs(25);

/// What a collapsed sleep waits instead. Long enough that a retry loop reached
/// through one cannot spin, short enough to leave the test sub-second.
const COLLAPSED_SLEEP: Duration = Duration::from_millis(200);

/// Wraps a platform and collapses its long sleeps. Every other capability is
/// the inner platform's.
#[derive(Debug, Clone)]
pub struct ShortDeadlinePlatform<T> {
	inner: T,
}

impl<T> ShortDeadlinePlatform<T> {
	/// Wraps `inner`.
	pub const fn new(inner: T) -> Self {
		ShortDeadlinePlatform { inner }
	}
}

impl<T: PlatformRef> PlatformRef for ShortDeadlinePlatform<T> {
	type Delay = T::Delay;
	type Instant = T::Instant;
	type MultiStream = T::MultiStream;
	type Stream = T::Stream;
	type ReadWriteAccess<'a> = T::ReadWriteAccess<'a>;
	type StreamErrorRef<'a> = T::StreamErrorRef<'a>;
	type StreamConnectFuture = T::StreamConnectFuture;
	type MultiStreamConnectFuture = T::MultiStreamConnectFuture;
	type StreamUpdateFuture<'a> = T::StreamUpdateFuture<'a>;
	type NextSubstreamFuture<'a> = T::NextSubstreamFuture<'a>;

	fn now_from_unix_epoch(&self) -> Duration {
		self.inner.now_from_unix_epoch()
	}

	fn now(&self) -> Self::Instant {
		self.inner.now()
	}

	fn fill_random_bytes(&self, buffer: &mut [u8]) {
		self.inner.fill_random_bytes(buffer)
	}

	/// Collapses a sleep of [`LONG_SLEEP`] or more; passes anything shorter
	/// through untouched.
	fn sleep(&self, duration: Duration) -> Self::Delay {
		let duration = if duration >= LONG_SLEEP { COLLAPSED_SLEEP } else { duration };
		self.inner.sleep(duration)
	}

	/// Left alone: smoldot arms the mode-decision deadline with `sleep`, and
	/// shortening a deadline expressed as an instant would need this platform's
	/// clock to disagree with [`PlatformRef::now`].
	fn sleep_until(&self, when: Self::Instant) -> Self::Delay {
		self.inner.sleep_until(when)
	}

	fn spawn_task(&self, task_name: Cow<str>, task: impl Future<Output = ()> + Send + 'static) {
		self.inner.spawn_task(task_name, task)
	}

	fn log<'a>(
		&self,
		log_level: LogLevel,
		log_target: &'a str,
		message: &'a str,
		key_values: impl Iterator<Item = (&'a str, &'a dyn core::fmt::Display)>,
	) {
		self.inner.log(log_level, log_target, message, key_values)
	}

	fn client_name(&'_ self) -> Cow<'_, str> {
		self.inner.client_name()
	}

	fn client_version(&'_ self) -> Cow<'_, str> {
		self.inner.client_version()
	}

	fn supports_connection_type(&self, connection_type: ConnectionType) -> bool {
		self.inner.supports_connection_type(connection_type)
	}

	fn connect_stream(&self, address: Address) -> Self::StreamConnectFuture {
		self.inner.connect_stream(address)
	}

	fn connect_multistream(&self, address: MultiStreamAddress) -> Self::MultiStreamConnectFuture {
		self.inner.connect_multistream(address)
	}

	fn open_out_substream(&self, connection: &mut Self::MultiStream) {
		self.inner.open_out_substream(connection)
	}

	fn next_substream<'a>(
		&self,
		connection: &'a mut Self::MultiStream,
	) -> Self::NextSubstreamFuture<'a> {
		self.inner.next_substream(connection)
	}

	fn read_write_access<'a>(
		&self,
		stream: Pin<&'a mut Self::Stream>,
	) -> Result<Self::ReadWriteAccess<'a>, Self::StreamErrorRef<'a>> {
		self.inner.read_write_access(stream)
	}

	fn wait_read_write_again<'a>(
		&self,
		stream: Pin<&'a mut Self::Stream>,
	) -> Self::StreamUpdateFuture<'a> {
		self.inner.wait_read_write_again(stream)
	}
}
