//! Trusted native Media bridge: the optional capture/RTC/compositor backend a
//! native host installs per product execution.
//!
//! Callbacks use canonical platform types directly. UniFFI validates bounded
//! byte values before publishing events to the core.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use futures::stream::{BoxStream, StreamExt};
use truapi::latest;

use crate::platform::{
    MediaBackendCapabilities, MediaBackendCommand, MediaBackendEvent, MediaBackendResponse,
    ProductContext,
};

/// Finite, redacted failures on the trusted native Media bridge.
#[derive(Debug, Clone, Copy, thiserror::Error, uniffi::Error)]
pub enum NativeMediaError {
    /// The event sink or backend subscription is already closed.
    #[error("media backend closed")]
    Closed,
    /// The bounded event queue overflowed; the stream terminates.
    #[error("media event overflow")]
    EventOverflow,
    /// Any other backend failure, redacted before it reaches the core.
    #[error("media backend failure")]
    BackendFailure,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for NativeMediaError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::BackendFailure
    }
}

impl From<NativeMediaError> for latest::GenericError {
    fn from(error: NativeMediaError) -> Self {
        Self { reason: error.to_string() }
    }
}

/// Complete, trusted capture/RTC/compositor adapter. Callbacks must not log
/// descriptions, candidates, device identifiers, or relay credentials.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait NativeMediaCallbacks: Send + Sync {
    /// Probe availability without prompting or capturing.
    async fn capabilities(
        &self,
        product: ProductContext,
    ) -> Result<MediaBackendCapabilities, NativeMediaError>;

    /// Start a live stream, publishing the current viewport first. Return
    /// promptly; the sink may be called from any thread, including this call.
    fn subscribe(
        &self,
        product: ProductContext,
        runtime_id: u64,
        event_sink: Arc<NativeMediaEventSink>,
    ) -> Result<(), NativeMediaError>;

    /// Capture may await a picker, but cancellation and teardown must remain
    /// independently actionable while another command is pending.
    async fn command(
        &self,
        product: ProductContext,
        runtime_id: u64,
        command: MediaBackendCommand,
    ) -> Result<MediaBackendResponse, NativeMediaError>;

    /// Release the event subscription; idempotent, prompt, and nonblocking.
    /// Runtime resources are released separately by CloseRuntime.
    fn unsubscribe(&self, runtime_id: u64);
}

const NATIVE_MEDIA_EVENT_CAPACITY: usize = 128;

#[derive(Default)]
struct NativeMediaQueue {
    events: std::collections::VecDeque<crate::platform::MediaBackendEvent>,
    closed: bool,
    terminal: Option<NativeMediaError>,
}

/// Trusted live event publisher scoped to one product runtime. Closing the
/// subscription invalidates every retained copy of this sink.
#[derive(uniffi::Object, Default)]
pub struct NativeMediaEventSink {
    queue: Mutex<NativeMediaQueue>,
    waker: futures::task::AtomicWaker,
}

#[uniffi::export]
impl NativeMediaEventSink {
    /// Publish one private event. Overflow terminates the stream instead of
    /// silently dropping state; no further event can be accepted.
    pub fn publish(
        &self,
        event: MediaBackendEvent,
    ) -> Result<(), NativeMediaError> {
        let mut queue = self.queue.lock().unwrap_or_else(|p| p.into_inner());
        if queue.closed {
            return Err(NativeMediaError::Closed);
        }
        let result = if queue.events.len() == NATIVE_MEDIA_EVENT_CAPACITY {
            queue.events.clear();
            queue.closed = true;
            queue.terminal = Some(NativeMediaError::EventOverflow);
            Err(NativeMediaError::EventOverflow)
        } else {
            queue.events.push_back(event);
            Ok(())
        };
        drop(queue);
        self.waker.wake();
        result
    }
}

struct NativeMediaStream {
    sink: Arc<NativeMediaEventSink>,
    callbacks: Option<Arc<dyn NativeMediaCallbacks>>,
    runtime_id: u64,
}

impl NativeMediaStream {
    fn close(&mut self) {
        {
            let mut queue = self.sink.queue.lock().unwrap_or_else(|p| p.into_inner());
            queue.closed = true;
            queue.events.clear();
        }
        if let Some(callbacks) = self.callbacks.take() {
            callbacks.unsubscribe(self.runtime_id);
        }
    }
}

impl futures::Stream for NativeMediaStream {
    type Item = Result<crate::platform::MediaBackendEvent, latest::GenericError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.sink.waker.register(cx.waker());
        let mut queue = self.sink.queue.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(error) = queue.terminal.take() {
            drop(queue);
            self.close();
            return std::task::Poll::Ready(Some(Err(error.into())));
        }
        if let Some(event) = queue.events.pop_front() {
            return std::task::Poll::Ready(Some(Ok(event)));
        }
        if queue.closed {
            std::task::Poll::Ready(None)
        } else {
            std::task::Poll::Pending
        }
    }
}

impl Drop for NativeMediaStream {
    fn drop(&mut self) {
        self.close();
    }
}

pub(super) struct MediaCallbackPlatform {
    pub(super) callbacks: Arc<dyn NativeMediaCallbacks>,
}

#[async_trait]
impl crate::platform::MediaPlatform for MediaCallbackPlatform {
    async fn media_backend_capabilities(
        &self,
        product: &ProductContext,
    ) -> Result<crate::platform::MediaBackendCapabilities, latest::GenericError> {
        self.callbacks.capabilities(product.clone()).await.map_err(Into::into)
    }

    fn media_backend_events(
        &self,
        product: &ProductContext,
        runtime_id: u64,
    ) -> BoxStream<'static, Result<crate::platform::MediaBackendEvent, latest::GenericError>> {
        let sink = Arc::new(NativeMediaEventSink::default());
        let stream = NativeMediaStream {
            sink: sink.clone(),
            callbacks: Some(self.callbacks.clone()),
            runtime_id,
        };
        if let Err(error) = self.callbacks.subscribe(product.clone(), runtime_id, sink.clone()) {
            let mut queue = sink.queue.lock().unwrap_or_else(|p| p.into_inner());
            queue.events.clear();
            queue.closed = true;
            queue.terminal = Some(error);
        }
        stream.boxed()
    }

    async fn media_backend_command(
        &self,
        product: &ProductContext,
        runtime_id: u64,
        command: crate::platform::MediaBackendCommand,
    ) -> Result<crate::platform::MediaBackendResponse, latest::GenericError> {
        self.callbacks.command(product.clone(), runtime_id, command).await.map_err(Into::into)
    }
}

