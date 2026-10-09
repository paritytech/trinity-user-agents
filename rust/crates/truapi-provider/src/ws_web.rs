//! Remote WebSocket JSON-RPC backend for `wasm32` (browser) targets.
//!
//! Mirrors the native backend's contract over the browser's `WebSocket`: a
//! raw string pipe whose responses stream ends when the socket dies. wasm is
//! single-threaded, so `SendWrapper` satisfies the trait's `Send + Sync`
//! bounds without real cross-thread use — calling a connection from another
//! thread would panic, and no such thread exists in this environment.

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};
use std::rc::Rc;

use crate::platform::JsonRpcConnection;
use futures::channel::{mpsc, oneshot};
use futures::stream::{BoxStream, StreamExt};
use parking_lot::Mutex;
use send_wrapper::SendWrapper;
use url::Url;

use crate::error::ProviderError;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, Event, MessageEvent, WebSocket};

/// The JS event callbacks kept alive for the socket's lifetime
/// (onmessage, onopen, onerror, onclose).
type SocketCallbacks = (
    Closure<dyn FnMut(MessageEvent)>,
    Closure<dyn FnMut(Event)>,
    Closure<dyn FnMut(Event)>,
    Closure<dyn FnMut(CloseEvent)>,
);

/// Own the DOM registrations for the entire socket lifetime, including while
/// the handshake future is pending or being cancelled.
struct Socket {
    socket: WebSocket,
    _callbacks: SocketCallbacks,
}

impl Socket {
    fn close(&self) {
        // Detach before closing or dropping any Closure: the browser may still
        // deliver queued events after close(), including a failed handshake.
        self.socket.set_onmessage(None);
        self.socket.set_onopen(None);
        self.socket.set_onerror(None);
        self.socket.set_onclose(None);
        if !matches!(
            self.socket.ready_state(),
            WebSocket::CLOSING | WebSocket::CLOSED
        ) {
            let _ = self.socket.close();
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.close();
    }
}

/// The connect future must be `Send`, but only accesses this on its JS thread.
type StagedSocket = SendWrapper<Socket>;

/// Open a browser WebSocket connection to `url`.
pub async fn connect(url: Url) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
    let (staged, handshake_rx, responses_tx, responses_rx) = open_socket(&url)?;

    match handshake_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(reason)) => {
            return Err(ProviderError::Handshake {
                url: crate::error::redacted(&url),
                reason,
            });
        }
        Err(_) => {
            return Err(ProviderError::Handshake {
                url: crate::error::redacted(&url),
                reason: "handshake abandoned".to_owned(),
            });
        }
    }

    Ok(Box::new(WebWsConnection {
        socket: staged,
        responses_close: responses_tx,
        responses: Mutex::new(Some(responses_rx.boxed())),
        closed: AtomicBool::new(false),
    }))
}

/// Create the socket and wire its callbacks.
///
/// Synchronous on purpose: every non-`Send` JS value is created and wrapped
/// here so the awaiting caller only holds `Send` state.
#[allow(clippy::type_complexity)]
fn open_socket(
    url: &Url,
) -> Result<
    (
        StagedSocket,
        oneshot::Receiver<Result<(), String>>,
        mpsc::UnboundedSender<String>,
        mpsc::UnboundedReceiver<String>,
    ),
    ProviderError,
> {
    let socket = WebSocket::new(url.as_str()).map_err(|err| ProviderError::Transport {
        reason: format!("WebSocket creation for {url} failed: {err:?}"),
    })?;
    socket.set_binary_type(BinaryType::Arraybuffer);

    // Unbounded on purpose: the browser `onmessage` callback cannot apply
    // backpressure, so a bound would force dropping inbound frames and break the
    // consumer's id correlation. Depth is governed by the consumer draining the
    // responses stream.
    let (responses_tx, responses_rx) = mpsc::unbounded::<String>();
    // Resolved exactly once by whichever of onopen/onerror/onclose fires
    // first, so the handshake can be awaited.
    let handshake = Rc::new(RefCell::new(None::<oneshot::Sender<Result<(), String>>>));
    let (handshake_tx, handshake_rx) = oneshot::channel();
    *handshake.borrow_mut() = Some(handshake_tx);

    let onmessage = {
        let responses_tx = responses_tx.clone();
        Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            if let Some(text) = event.data().as_string() {
                let _ = responses_tx.unbounded_send(text);
            } else if let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                match String::from_utf8(bytes) {
                    Ok(text) => {
                        let _ = responses_tx.unbounded_send(text);
                    }
                    Err(_) => tracing::warn!("dropping non-UTF-8 binary WebSocket frame"),
                }
            }
        })
    };
    let onopen = {
        let handshake = Rc::clone(&handshake);
        Closure::<dyn FnMut(Event)>::new(move |_| {
            let sender = handshake.borrow_mut().take();
            if let Some(sender) = sender {
                let _ = sender.send(Ok(()));
            }
        })
    };
    let onerror = {
        let handshake = Rc::clone(&handshake);
        Closure::<dyn FnMut(Event)>::new(move |_| {
            let sender = handshake.borrow_mut().take();
            if let Some(sender) = sender {
                let _ = sender.send(Err("WebSocket error during handshake".to_owned()));
            }
        })
    };
    let onclose = {
        let handshake = Rc::clone(&handshake);
        let responses_tx = responses_tx.clone();
        Closure::<dyn FnMut(CloseEvent)>::new(move |event: CloseEvent| {
            let sender = handshake.borrow_mut().take();
            if let Some(sender) = sender {
                let _ = sender.send(Err(format!(
                    "WebSocket closed during handshake (code {})",
                    event.code()
                )));
            }
            // Ending the channel ends the responses stream — the disconnect
            // signal consumers rely on.
            responses_tx.close_channel();
        })
    };
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));

    Ok((
        SendWrapper::new(Socket {
            socket,
            _callbacks: (onmessage, onopen, onerror, onclose),
        }),
        handshake_rx,
        responses_tx,
        responses_rx,
    ))
}

/// A live browser WebSocket connection exposed as a raw JSON-RPC pipe.
struct WebWsConnection {
    socket: StagedSocket,
    /// Sender half of the responses channel, kept to end the stream on close.
    responses_close: mpsc::UnboundedSender<String>,
    responses: Mutex<Option<BoxStream<'static, String>>>,
    closed: AtomicBool,
}

impl JsonRpcConnection for WebWsConnection {
    fn send(&self, request: String) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        if let Err(err) = self.socket.socket.send_with_str(&request) {
            // A send failure on a browser WebSocket means the socket is dead.
            // End the responses stream so the consumer — which correlates by
            // id — sees a disconnect instead of hanging on a request that will
            // never be answered.
            tracing::warn!("WebSocket send failed: {err:?}");
            self.closed.store(true, Ordering::SeqCst);
            self.responses_close.close_channel();
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        match self.responses.lock().take() {
            Some(responses) => responses,
            None => futures::stream::empty().boxed(),
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.socket.close();
        self.responses_close.close_channel();
        self.responses.lock().take();
    }
}

impl Drop for WebWsConnection {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::task::{Context, Poll};

    use futures::FutureExt;
    use futures::task::{ArcWake, waker};
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;

    #[cfg(not(feature = "js"))]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    fn test_url() -> Url {
        Url::parse("ws://127.0.0.1:1").unwrap()
    }

    fn assert_detached(socket: &WebSocket) {
        assert!(socket.onopen().is_none());
        assert!(socket.onmessage().is_none());
        assert!(socket.onerror().is_none());
        assert!(socket.onclose().is_none());
        // Exercise the real browser EventTarget after Rust has released the
        // callbacks. Queued browser events must no longer enter dropped WASM.
        for event in ["open", "message", "error", "close"] {
            socket.dispatch_event(&Event::new(event).unwrap()).unwrap();
        }
    }

    #[wasm_bindgen_test]
    fn cancelling_handshake_detaches_browser_callbacks() {
        let (staged, handshake, responses_tx, mut responses) = open_socket(&test_url()).unwrap();
        let socket = staged.socket.clone();
        drop(staged);
        assert_detached(&socket);
        assert!(matches!(handshake.now_or_never(), Some(Err(_))));
        drop(responses_tx);
        assert_eq!(responses.next().now_or_never(), Some(None));
    }

    #[wasm_bindgen_test]
    fn failed_handshake_detaches_browser_callbacks() {
        for event in [
            Event::new("error").unwrap(),
            CloseEvent::new("close").unwrap().unchecked_into(),
        ] {
            let (staged, handshake, _, _) = open_socket(&test_url()).unwrap();
            let socket = staged.socket.clone();
            socket.dispatch_event(&event).unwrap();
            assert!(matches!(handshake.now_or_never(), Some(Ok(Err(_)))));
            drop(staged);
            assert_detached(&socket);
        }
    }

    #[wasm_bindgen_test]
    fn closing_or_dropping_connection_detaches_browser_callbacks() {
        for explicit_close in [false, true] {
            let (staged, handshake, responses_tx, responses_rx) = open_socket(&test_url()).unwrap();
            let socket = staged.socket.clone();
            socket.dispatch_event(&Event::new("open").unwrap()).unwrap();
            assert!(matches!(handshake.now_or_never(), Some(Ok(Ok(())))));
            let connection = WebWsConnection {
                socket: staged,
                responses_close: responses_tx,
                responses: Mutex::new(Some(responses_rx.boxed())),
                closed: AtomicBool::new(false),
            };
            let mut responses = connection.responses();
            if explicit_close {
                connection.close();
                connection.close();
                assert_detached(&socket);
                assert_eq!(responses.next().now_or_never(), Some(None));
            }
            drop(connection);
            assert_detached(&socket);
            assert_eq!(responses.next().now_or_never(), Some(None));
        }
    }

    struct DispatchErrorOnWake {
        socket: SendWrapper<WebSocket>,
        completed: AtomicBool,
    }

    impl ArcWake for DispatchErrorOnWake {
        fn wake_by_ref(this: &Arc<Self>) {
            // Invoke a second real WASM callback synchronously from the
            // handshake receiver's waker. Its borrow must already be released.
            this.socket
                .onerror()
                .unwrap()
                .call1(&JsValue::NULL, &Event::new("error").unwrap())
                .unwrap();
            this.completed.store(true, Ordering::SeqCst);
        }
    }

    #[wasm_bindgen_test]
    fn handshake_callback_releases_borrow_before_waking() {
        let (staged, mut handshake, _, _) = open_socket(&test_url()).unwrap();
        let observer = Arc::new(DispatchErrorOnWake {
            socket: SendWrapper::new(staged.socket.clone()),
            completed: AtomicBool::new(false),
        });
        let waker = waker(observer.clone());
        let mut cx = Context::from_waker(&waker);
        assert!(matches!(handshake.poll_unpin(&mut cx), Poll::Pending));
        staged
            .socket
            .dispatch_event(&Event::new("open").unwrap())
            .unwrap();
        assert!(observer.completed.load(Ordering::SeqCst));
        assert!(matches!(handshake.now_or_never(), Some(Ok(Ok(())))));
    }
}
