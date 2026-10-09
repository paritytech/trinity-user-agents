// Copyright 2019-2026 Parity Technologies (UK) Ltd.
// This file is dual-licensed as Apache-2.0 or GPL-3.0; see LICENSE-APACHE.
// Vendored from subxt-lightclient 0.50.1 (src/platform/wasm_socket.rs), used
// under Apache-2.0. Local changes ignore non-ArrayBuffer frames, wake outside
// the state lock, and detach DOM callbacks before closing the socket.

use futures::{io, prelude::*};
use parking_lot::Mutex;
use send_wrapper::SendWrapper;
use wasm_bindgen::{JsCast, prelude::*};

use std::{
    collections::VecDeque,
    pin::Pin,
    sync::Arc,
    task::Poll,
    task::{Context, Waker},
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to connect {0}")]
    ConnectionError(String),
}

/// Websocket for WASM environments.
///
/// This is a rust-based wrapper around browser's WebSocket API.
///
// Warning: It is not safe to have `Clone` on this structure.
pub struct WasmSocket {
    /// Inner data shared between `poll` and web_sys callbacks.
    inner: Arc<Mutex<InnerWasmSocket>>,
    /// This implements `Send` and panics if the value is accessed
    /// or dropped from another thread.
    ///
    /// This is safe in wasm environments.
    socket: SendWrapper<web_sys::WebSocket>,
    /// In memory callbacks to handle messages from the browser socket.
    _callbacks: SendWrapper<Callbacks>,
}

/// The state of the [`WasmSocket`].
#[derive(PartialEq, Eq, Clone, Copy)]
enum ConnectionState {
    /// Initial state of the socket.
    Connecting,
    /// Socket is fully opened.
    Opened,
    /// Socket is closed.
    Closed,
    /// Error reported by callbacks.
    Error,
}

struct InnerWasmSocket {
    /// The state of the connection.
    state: ConnectionState,
    /// Data buffer for the socket.
    data: VecDeque<u8>,
    /// Waker from `poll_read` / `poll_write`.
    waker: Option<Waker>,
}

/// Registered callbacks of the [`WasmSocket`].
///
/// These need to be kept around until the socket is dropped.
type Callbacks = (
    Closure<dyn FnMut()>,
    Closure<dyn FnMut(web_sys::MessageEvent)>,
    Closure<dyn FnMut(web_sys::Event)>,
    Closure<dyn FnMut(web_sys::CloseEvent)>,
);

impl WasmSocket {
    /// Establish a WebSocket connection.
    ///
    /// The error is a string representing the browser error.
    /// Visit [MDN Documentation](https://developer.mozilla.org/en-US/docs/Web/API/WebSocket/WebSocket#exceptions_thrown)
    /// for more info.
    pub fn new(addr: &str) -> Result<Self, Error> {
        let socket = match web_sys::WebSocket::new(addr) {
            Ok(socket) => socket,
            Err(err) => return Err(Error::ConnectionError(format!("{err:?}"))),
        };

        socket.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let inner = Arc::new(Mutex::new(InnerWasmSocket {
            state: ConnectionState::Connecting,
            data: VecDeque::with_capacity(16384),
            waker: None,
        }));

        let open_callback = Closure::<dyn FnMut()>::new({
            let inner = inner.clone();
            move || {
                let mut inner = inner.lock();
                inner.state = ConnectionState::Opened;

                let waker = inner.waker.take();
                drop(inner);
                if let Some(waker) = waker {
                    waker.wake();
                }
            }
        });
        socket.set_onopen(Some(open_callback.as_ref().unchecked_ref()));

        let message_callback = Closure::<dyn FnMut(_)>::new({
            let inner = inner.clone();
            move |event: web_sys::MessageEvent| {
                // A compliant libp2p peer sends only binary frames. Ignore
                // anything else rather than panicking inside a browser callback,
                // which would abort the whole light client; the libp2p handshake
                // then fails cleanly downstream on the missing bytes.
                let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() else {
                    tracing::warn!("ignoring non-ArrayBuffer WebSocket frame from libp2p peer");
                    return;
                };

                let mut inner = inner.lock();
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                inner.data.extend(bytes);

                let waker = inner.waker.take();
                drop(inner);
                if let Some(waker) = waker {
                    waker.wake();
                }
            }
        });
        socket.set_onmessage(Some(message_callback.as_ref().unchecked_ref()));

        let error_callback = Closure::<dyn FnMut(_)>::new({
            let inner = inner.clone();
            move |_event: web_sys::Event| {
                // Callback does not provide useful information, signal it back to the stream.
                let mut inner = inner.lock();
                inner.state = ConnectionState::Error;

                let waker = inner.waker.take();
                drop(inner);
                if let Some(waker) = waker {
                    waker.wake();
                }
            }
        });
        socket.set_onerror(Some(error_callback.as_ref().unchecked_ref()));

        let close_callback = Closure::<dyn FnMut(_)>::new({
            let inner = inner.clone();
            move |_event: web_sys::CloseEvent| {
                let mut inner = inner.lock();
                inner.state = ConnectionState::Closed;

                let waker = inner.waker.take();
                drop(inner);
                if let Some(waker) = waker {
                    waker.wake();
                }
            }
        });
        socket.set_onclose(Some(close_callback.as_ref().unchecked_ref()));

        let callbacks = (
            open_callback,
            message_callback,
            error_callback,
            close_callback,
        );

        Ok(Self {
            inner,
            socket: SendWrapper::new(socket),
            _callbacks: SendWrapper::new(callbacks),
        })
    }
}

impl AsyncRead for WasmSocket {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut inner = self.inner.lock();
        inner.waker = Some(cx.waker().clone());

        if self.socket.ready_state() == web_sys::WebSocket::CONNECTING {
            return Poll::Pending;
        }

        match inner.state {
            ConnectionState::Error => Poll::Ready(Err(io::Error::other("Socket error"))),
            ConnectionState::Closed => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            ConnectionState::Connecting => Poll::Pending,
            ConnectionState::Opened => {
                if inner.data.is_empty() {
                    return Poll::Pending;
                }

                let n = inner.data.len().min(buf.len());
                for k in buf.iter_mut().take(n) {
                    *k = inner.data.pop_front().expect("Buffer non empty; qed");
                }
                Poll::Ready(Ok(n))
            }
        }
    }
}

impl AsyncWrite for WasmSocket {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut inner = self.inner.lock();
        inner.waker = Some(cx.waker().clone());

        match inner.state {
            ConnectionState::Error => Poll::Ready(Err(io::Error::other("Socket error"))),
            ConnectionState::Closed => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            ConnectionState::Connecting => Poll::Pending,
            ConnectionState::Opened => match self.socket.send_with_u8_array(buf) {
                Ok(()) => Poll::Ready(Ok(buf.len())),
                Err(err) => Poll::Ready(Err(io::Error::other(format!("Write error: {err:?}")))),
            },
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        if self.socket.ready_state() == web_sys::WebSocket::CLOSED {
            return Poll::Ready(Ok(()));
        }

        if self.socket.ready_state() != web_sys::WebSocket::CLOSING {
            let _ = self.socket.close();
        }

        let mut inner = self.inner.lock();
        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl Drop for WasmSocket {
    fn drop(&mut self) {
        // close() queues browser events; none may retain a callback whose Rust
        // owner is about to be dropped.
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onerror(None);
        self.socket.set_onclose(None);

        if !matches!(
            self.socket.ready_state(),
            web_sys::WebSocket::CLOSING | web_sys::WebSocket::CLOSED
        ) {
            let _ = self.socket.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::task::{ArcWake, waker};
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;

    #[cfg(not(any(feature = "js", feature = "ws")))]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    fn binary_message_event() -> web_sys::MessageEvent {
        let init = web_sys::MessageEventInit::new();
        init.set_data(&js_sys::Uint8Array::from(&[1, 2, 3][..]).buffer());
        web_sys::MessageEvent::new_with_event_init_dict("message", &init).unwrap()
    }

    struct InspectOnWake {
        inner: Arc<Mutex<InnerWasmSocket>>,
        wakes: AtomicUsize,
    }

    impl ArcWake for InspectOnWake {
        fn wake_by_ref(this: &Arc<Self>) {
            // A waker may synchronously re-enter the task polling this socket.
            // try_lock makes the regression fail rather than deadlock in WASM.
            assert!(
                this.inner.try_lock().is_some(),
                "socket state locked during wake"
            );
            this.wakes.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[wasm_bindgen_test]
    fn browser_callbacks_release_state_before_waking() {
        let socket = WasmSocket::new("ws://127.0.0.1:1").unwrap();
        let observer = Arc::new(InspectOnWake {
            inner: socket.inner.clone(),
            wakes: AtomicUsize::new(0),
        });
        let events = [
            web_sys::Event::new("open").unwrap(),
            binary_message_event().unchecked_into(),
            web_sys::Event::new("error").unwrap(),
            web_sys::CloseEvent::new("close").unwrap().unchecked_into(),
        ];
        for (index, event) in events.iter().enumerate() {
            socket.inner.lock().waker = Some(waker(observer.clone()));
            socket.socket.dispatch_event(event).unwrap();
            assert_eq!(observer.wakes.load(Ordering::SeqCst), index + 1);
        }
        let inner = socket.inner.lock();
        assert_eq!(inner.data.iter().copied().collect::<Vec<_>>(), [1, 2, 3]);
        assert!(inner.state == ConnectionState::Closed);
    }

    #[wasm_bindgen_test]
    fn dropping_socket_detaches_and_releases_browser_callbacks() {
        let socket = WasmSocket::new("ws://127.0.0.1:1").unwrap();
        let browser_socket = (*socket.socket).clone();
        let inner = Arc::downgrade(&socket.inner);
        drop(socket);

        assert!(browser_socket.onopen().is_none());
        assert!(browser_socket.onmessage().is_none());
        assert!(browser_socket.onerror().is_none());
        assert!(browser_socket.onclose().is_none());
        assert!(
            inner.upgrade().is_none(),
            "callbacks must not leak their state"
        );
        for event in ["open", "message", "error", "close"] {
            browser_socket
                .dispatch_event(&web_sys::Event::new(event).unwrap())
                .unwrap();
        }
    }
}
