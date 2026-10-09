//! Subscription lifecycle management.
//!
//! Tracks active subscriptions (start/receive/stop/interrupt) and handles
//! cleanup when either side terminates. Each registered subscription drives
//! its stream on a caller-supplied [`Spawner`]; the manager itself never
//! creates threads or runtimes.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::future::{BoxFuture, Either, select};
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use parity_scale_codec::{Decode, DecodeLimit, Encode};
use truapi::CallError;

use crate::frame::{
    IdFactory, MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_START, MESSAGE_TYPE_STOP,
    PROTOCOL_ERROR_KEY, Payload, ProtocolErrorV1, ProtocolMessage, VersionedProtocolError,
    encode_clean_interrupt,
};
use crate::generated::wire_table::MethodIds;
use crate::protocol_error::decode_protocol_error_payload;
use crate::transport::Transport;

type StopFn = Box<dyn FnOnce() + Send>;

/// Spawns a subscription-driving future onto the caller's runtime. The
/// future is `Send` because the inner [`SubscriptionStream`] is a
/// `BoxStream<'static, _>` and every captured value the manager threads
/// through it is also `Send`. Each platform bridge supplies an
/// implementation that hands the future to the runtime driving its transport
/// (`tokio::spawn`, `wasm_bindgen_futures::spawn_local`, ...).
///
/// An implementation must not panic. The core spawns teardown work from
/// `Drop`, where a panic during unwinding aborts the process, so a spawner
/// whose runtime is already gone drops the future and reports the failure
/// instead.
pub type Spawner = Arc<dyn Fn(BoxFuture<'static, ()>) + Send + Sync>;

/// Convenience spawner for tests and embedders that don't yet wire a
/// real runtime: starts a fresh OS thread per subscription and drives the
/// future with `futures::executor::block_on`. Not available on wasm32 since
/// the platform has no threads.
#[cfg(not(target_arch = "wasm32"))]
pub fn thread_per_subscription_spawner() -> Spawner {
    Arc::new(|fut: BoxFuture<'static, ()>| {
        std::thread::spawn(move || futures::executor::block_on(fut));
    })
}

/// One yielded value of a subscription stream after SCALE-encoding.
pub enum SubscriptionOutput {
    /// A regular subscription item to deliver as a `_receive` frame.
    Item(Vec<u8>),
    /// Stream-initiated termination delivered as an `_interrupt` frame.
    Interrupt(Vec<u8>),
}

/// Boxed stream of [`SubscriptionOutput`] consumed by the dispatcher.
pub type SubscriptionStream = BoxStream<'static, SubscriptionOutput>;

/// Wrap a host-side subscription into the SCALE-encoded
/// [`SubscriptionStream`] that the dispatcher delivers to the transport.
///
/// `Item` is the versioned wrapper for each emitted value (e.g.
/// `versioned::account::HostAccountConnectionStatusSubscribeItem`) and
/// `Interrupt` the value that ends the stream. The generated dispatcher calls
/// this with both inferred from the host trait return.
pub fn subscription_stream<Item, Interrupt, S>(stream: S) -> SubscriptionStream
where
    Item: Encode + 'static,
    Interrupt: Encode + 'static,
    S: futures::Stream<Item = Result<Item, Interrupt>> + Send + 'static,
{
    Box::pin(stream.map(|item| {
        match item {
            Ok(item) => SubscriptionOutput::Item(item.encode()),
            Err(interrupt) => SubscriptionOutput::Interrupt(subscription_interrupt(interrupt)),
        }
    }))
}

/// Encode the `Interrupt` payload that ends a subscription with `interrupt`.
///
/// The leg carries `Result<(), Interrupt>`, so a value rides in its `Err`
/// arm and a stream that ends without one sends
/// [`encode_clean_interrupt`]'s `Ok(())`.
pub fn subscription_interrupt<Interrupt>(interrupt: Interrupt) -> Vec<u8>
where
    Interrupt: Encode,
{
    Err::<(), Interrupt>(interrupt).encode()
}

/// Generation-stamped slot tracking the lifecycle of one subscription id.
/// `request_id` is client-controlled and may be reused or raced against a
/// `_stop`, so each reservation carries a monotonic generation and only the
/// owner of the current generation may transition or remove the slot.
enum Slot {
    /// Reserved by the dispatcher before its `_start` handler resolved.
    /// `cancelled` flips to `true` if a `_stop` arrives in that window so
    /// activation aborts instead of leaking an unstoppable stream.
    Pending { generation: u64, cancelled: bool },
    /// A live subscription with its cancellation handle.
    Live { generation: u64, cancel: StopFn },
}

/// Handle returned by [`SubscriptionManager::reserve`] and presented back to
/// [`SubscriptionManager::activate`]. Ties an activation to the exact
/// reservation it belongs to so a superseding `_start` for the same id
/// cannot be activated by a stale handler.
pub struct ReservationToken {
    request_id: String,
    generation: u64,
}

/// Manages active subscriptions on the server side.
pub struct SubscriptionManager {
    active: Arc<Mutex<HashMap<String, Slot>>>,
    next_generation: Arc<AtomicU64>,
    spawner: Spawner,
}

impl SubscriptionManager {
    /// Create an empty manager driven by `spawner`.
    pub fn new(spawner: Spawner) -> Self {
        Self {
            active: Arc::new(Mutex::new(HashMap::new())),
            next_generation: Arc::new(AtomicU64::new(0)),
            spawner,
        }
    }

    /// Reserve the slot for `request_id` before its subscription stream is
    /// available. Any live subscription already under that id is stopped and
    /// replaced (re-subscribe semantics). A `_stop` arriving before
    /// [`activate`](Self::activate) flips the reservation to cancelled.
    pub fn reserve(&self, request_id: String) -> ReservationToken {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let mut active = self.active.lock().unwrap();
        if let Some(Slot::Live { cancel, .. }) = active.insert(
            request_id.clone(),
            Slot::Pending {
                generation,
                cancelled: false,
            },
        ) {
            cancel();
        }
        ReservationToken {
            request_id,
            generation,
        }
    }

    /// Drop a reservation whose `_start` handler failed before producing a
    /// stream. No-op if the slot was superseded by a newer reservation.
    pub fn cancel_reservation(&self, token: ReservationToken) {
        let mut active = self.active.lock().unwrap();
        let owned = matches!(
            active.get(&token.request_id),
            Some(Slot::Pending { generation, .. }) if *generation == token.generation
        );
        if owned {
            active.remove(&token.request_id);
        }
    }

    /// Activate a reserved subscription with its stream, forwarding stream
    /// items as `_receive` frames until the stream ends or `_stop` is
    /// received. No-ops without starting the stream if the reservation was
    /// cancelled by a `_stop` or superseded by a newer reservation for the
    /// same id.
    pub fn activate(
        &self,
        token: ReservationToken,
        trait_id: u8,
        method_id: u8,
        mut stream: SubscriptionStream,
        transport: Arc<dyn Transport>,
    ) {
        let ReservationToken {
            request_id,
            generation,
        } = token;
        let rid = request_id.clone();
        let stream_transport = transport.clone();

        // Cancellation channel.
        let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel::<()>();

        // Transition the reserved slot to live, unless a `_stop` cancelled it
        // or a newer reservation superseded it while the handler resolved.
        {
            let mut active = self.active.lock().unwrap();
            match active.get_mut(&request_id) {
                Some(Slot::Pending {
                    generation: g,
                    cancelled,
                }) if *g == generation => {
                    if *cancelled {
                        active.remove(&request_id);
                        return;
                    }
                }
                _ => return,
            }
            active.insert(
                request_id.clone(),
                Slot::Live {
                    generation,
                    cancel: Box::new(move || {
                        let _ = cancel_tx.send(());
                    }),
                },
            );
        }

        let active = self.active.clone();

        let future: BoxFuture<'static, ()> = Box::pin(async move {
            let completed = {
                let mut cancel_rx = cancel_rx;
                loop {
                    match select(cancel_rx, stream.next()).await {
                        Either::Left((_cancelled, _next)) => break false,
                        Either::Right((item, next_cancel_rx)) => {
                            cancel_rx = next_cancel_rx;
                            match item {
                                Some(SubscriptionOutput::Item(value)) => {
                                    stream_transport.send(ProtocolMessage {
                                        request_id: rid.clone(),
                                        payload: Payload {
                                            trait_id,
                                            method_id,
                                            message_type: MESSAGE_TYPE_RECEIVE,
                                            value,
                                        },
                                    })
                                }
                                Some(SubscriptionOutput::Interrupt(value)) => {
                                    stream_transport.send(ProtocolMessage {
                                        request_id: rid.clone(),
                                        payload: Payload {
                                            trait_id,
                                            method_id,
                                            message_type: MESSAGE_TYPE_INTERRUPT,
                                            value,
                                        },
                                    });
                                    break false;
                                }
                                None => break true,
                            }
                        }
                    }
                }
            };

            // Only remove the slot if it still holds THIS generation; a
            // superseding reservation owns its own cleanup.
            let removed = {
                let mut active = active.lock().unwrap();
                let owned = matches!(
                    active.get(&request_id),
                    Some(Slot::Live { generation: g, .. }) if *g == generation
                );
                if owned {
                    active.remove(&request_id);
                }
                owned
            };

            if completed && removed {
                transport.send(ProtocolMessage {
                    request_id,
                    payload: Payload {
                        trait_id,
                        method_id,
                        message_type: MESSAGE_TYPE_INTERRUPT,
                        value: encode_clean_interrupt(),
                    },
                });
            }
        });

        (self.spawner)(future);
    }

    /// Convenience for callers that already hold the stream with no async gap
    /// between reservation and activation (tests and synchronous embedders).
    pub fn register(
        &self,
        request_id: String,
        trait_id: u8,
        method_id: u8,
        stream: SubscriptionStream,
        transport: Arc<dyn Transport>,
    ) {
        let token = self.reserve(request_id);
        self.activate(token, trait_id, method_id, stream, transport);
    }

    /// Handle a `_stop` frame from the product side. Cancels a live
    /// subscription, or marks a still-pending reservation cancelled so its
    /// in-flight activation aborts rather than leaking an unstoppable stream.
    pub fn handle_stop(&self, request_id: &str) {
        let mut active = self.active.lock().unwrap();
        match active.get_mut(request_id) {
            Some(Slot::Pending { cancelled, .. }) => {
                *cancelled = true;
            }
            Some(Slot::Live { .. }) => {
                if let Some(Slot::Live { cancel, .. }) = active.remove(request_id) {
                    cancel();
                }
            }
            None => {}
        }
    }

    /// Cancel and forget every pending or live subscription owned by this
    /// manager. Used when the product runtime is disposed, where no further
    /// frames should be emitted and platform resources must be released.
    pub fn cancel_all(&self) {
        let cancellations = {
            let mut active = self.active.lock().unwrap();
            active
                .drain()
                .filter_map(|(_, slot)| {
                    match slot {
                        Slot::Pending { .. } => None,
                        Slot::Live { cancel, .. } => Some(cancel),
                    }
                })
                .collect::<Vec<_>>()
        };
        for cancel in cancellations {
            cancel();
        }
    }
}

/// One frame routed to a live host-initiated stream. The product ends a
/// stream with `_interrupt`, whose `Result<(), CallError<E>>` payload says
/// which kind of end it is: `Ok(())` a clean completion, `Err(error)` a
/// failure carrying the method's own interrupt value.
enum HostInitiatedFrame {
    Item(Vec<u8>),
    /// The product ended the stream. The payload is decoded by the stream
    /// itself, which is the only place the method's interrupt type is known.
    Interrupt(Vec<u8>),
    Unsupported,
    /// A correlated protocol error this build cannot read. Terminal, because
    /// the peer has answered and will not answer again.
    UnknownProtocolError,
}

struct HostInitiatedSlot {
    ids: MethodIds,
    sender: mpsc::UnboundedSender<HostInitiatedFrame>,
}

struct HostInitiatedState {
    ids: IdFactory,
    active: HashMap<String, HostInitiatedSlot>,
    closed: bool,
}

/// Manages subscriptions that the native host opens into a product execution.
///
/// Host-owned ids use the reserved `h:` prefix. Product `_receive` and
/// `_interrupt` frames are routed by request id, while dropping the returned
/// stream sends `_stop` for only that render instance.
pub struct HostInitiatedSubscriptionManager {
    state: Arc<Mutex<HostInitiatedState>>,
}

impl Default for HostInitiatedSubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HostInitiatedSubscriptionManager {
    /// Create an empty host-initiated subscription manager.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HostInitiatedState {
                ids: IdFactory::new("h:"),
                active: HashMap::new(),
                closed: false,
            })),
        }
    }

    /// Start one typed subscription and send its `Start` frame to the
    /// product. `payload` is already the SCALE-encoded request wrapper's own
    /// bytes, constructed by the generated caller.
    pub fn start<Item, Interrupt>(
        &self,
        ids: MethodIds,
        payload: Vec<u8>,
        transport: Arc<dyn Transport>,
    ) -> truapi::Subscription<Item, CallError<Interrupt>>
    where
        Item: Decode + Send + Unpin + 'static,
        Interrupt: Decode + Send + Unpin + 'static,
    {
        let (sender, receiver) = mpsc::unbounded();
        let request_id = {
            let mut state = self
                .state
                .lock()
                .expect("host subscription state mutex poisoned");
            if state.closed {
                return truapi::Subscription::interrupted(CallError::HostFailure {
                    reason: "host-initiated subscriptions are closed".to_string(),
                });
            }
            let request_id = state.ids.next_id();
            state
                .active
                .insert(request_id.clone(), HostInitiatedSlot { ids, sender });
            request_id
        };

        transport.send(ProtocolMessage {
            request_id: request_id.clone(),
            payload: Payload {
                trait_id: ids.trait_id,
                method_id: ids.method_id,
                message_type: MESSAGE_TYPE_START,
                value: payload,
            },
        });

        truapi::Subscription::new(HostInitiatedSubscription::<Item, Interrupt> {
            request_id,
            ids,
            receiver,
            state: self.state.clone(),
            transport,
            terminated: false,
            marker: PhantomData,
        })
    }

    /// Route one product frame. Every `h:` id belongs to this manager and is
    /// consumed even when its subscription has already ended; any other frame
    /// is returned to the caller untouched.
    pub fn handle_message(&self, message: ProtocolMessage) -> Option<ProtocolMessage> {
        if !message.request_id.starts_with("h:") {
            return Some(message);
        }

        let mut state = self
            .state
            .lock()
            .expect("host subscription state mutex poisoned");
        let slot = state.active.get(&message.request_id)?;
        let key = (message.payload.trait_id, message.payload.method_id);
        // The protocol-error check MUST precede the trait guard. A protocol
        // error is addressed to the reserved trait, never to this slot's, so
        // guarding on the trait first would make the arm below dead code and
        // silently drop the frame that reports our start as unsupported.
        if key == PROTOCOL_ERROR_KEY {
            let frame = match decode_protocol_error_payload(&message.payload.value) {
                Ok(Some(VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
                    trait_id,
                    method_id,
                }))) => {
                    // Only OUR start frame going unsupported ends this render;
                    // an error about any other pair belongs to a different
                    // subscription.
                    if (trait_id, method_id) != (slot.ids.trait_id, slot.ids.method_id) {
                        return None;
                    }
                    HostInitiatedFrame::Unsupported
                }
                // A protocol error from a later build. Its shape is unknowable
                // here, so it cannot be attributed to a pair, but it is
                // correlated to this request id and the peer will not answer
                // again. Settling is what stops the stream waiting forever.
                Ok(None) => HostInitiatedFrame::UnknownProtocolError,
                // Unreachable in practice: `ProtocolMessage::decode` rejects a
                // malformed protocol-error payload at the frame boundary.
                Err(_) => return None,
            };
            let sender = slot.sender.clone();
            let _ = sender.unbounded_send(frame);
            state.active.remove(&message.request_id);
            return None;
        }
        if message.payload.trait_id != slot.ids.trait_id
            || message.payload.method_id != slot.ids.method_id
        {
            return None;
        }
        if message.payload.message_type == MESSAGE_TYPE_RECEIVE {
            let sender = slot.sender.clone();
            drop(state);
            let _ = sender.unbounded_send(HostInitiatedFrame::Item(message.payload.value));
        } else if message.payload.message_type == MESSAGE_TYPE_INTERRUPT {
            // The payload is forwarded undecoded: its `Result<(), CallError<E>>`
            // shape is method-specific and this manager is generic over the
            // item type alone. Deliver the terminal before dropping the
            // sender, so the stream never reports a silent end.
            let sender = slot.sender.clone();
            let _ = sender.unbounded_send(HostInitiatedFrame::Interrupt(message.payload.value));
            state.active.remove(&message.request_id);
        }
        None
    }

    /// Close every active host-initiated stream without sending new frames.
    pub fn close(&self) {
        let mut state = self
            .state
            .lock()
            .expect("host subscription state mutex poisoned");
        state.closed = true;
        state.active.clear();
    }
}

struct HostInitiatedSubscription<Item, Interrupt> {
    request_id: String,
    ids: MethodIds,
    receiver: mpsc::UnboundedReceiver<HostInitiatedFrame>,
    state: Arc<Mutex<HostInitiatedState>>,
    transport: Arc<dyn Transport>,
    terminated: bool,
    marker: PhantomData<(Item, Interrupt)>,
}

impl<Item, Interrupt> HostInitiatedSubscription<Item, Interrupt> {
    fn stop(&mut self) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        let removed = self
            .state
            .lock()
            .expect("host subscription state mutex poisoned")
            .active
            .remove(&self.request_id)
            .is_some();
        if removed {
            self.transport.send(ProtocolMessage {
                request_id: self.request_id.clone(),
                payload: Payload {
                    trait_id: self.ids.trait_id,
                    method_id: self.ids.method_id,
                    message_type: MESSAGE_TYPE_STOP,
                    value: Vec::new(),
                },
            });
        }
    }
}

/// Nesting a product-supplied subscription item may reach before it is refused.
///
/// Recursive payloads such as a custom renderer tree would otherwise decode
/// until the thread's stack is exhausted, which aborts the process rather than
/// failing the call. Far above any nesting the protocol's own types need.
const MAX_SUBSCRIPTION_DECODE_DEPTH: u32 = 64;

impl<Item, Interrupt> Stream for HostInitiatedSubscription<Item, Interrupt>
where
    Item: Decode + Unpin,
    Interrupt: Decode + Unpin,
{
    type Item = Result<Item, CallError<Interrupt>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.receiver).poll_next(cx) {
            Poll::Ready(Some(HostInitiatedFrame::Item(bytes))) => {
                let mut input = &bytes[..];
                match Item::decode_with_depth_limit(MAX_SUBSCRIPTION_DECODE_DEPTH, &mut input) {
                    Ok(item) if input.is_empty() => Poll::Ready(Some(Ok(item))),
                    Ok(_) | Err(_) => {
                        // The peer sees a bare stop frame, so this is the only
                        // record of why the item was refused. The codec's own
                        // error chains to kilobytes, so it is deliberately not
                        // included.
                        tracing::warn!(
                            request_id = %self.request_id,
                            "refused a host subscription item: undecodable or nested past the limit"
                        );
                        self.stop();
                        Poll::Ready(Some(Err(CallError::MalformedFrame {
                            reason: "host-initiated subscription item did not decode".to_string(),
                        })))
                    }
                }
            }
            Poll::Ready(Some(HostInitiatedFrame::Interrupt(bytes))) => {
                self.terminated = true;
                let mut input = &bytes[..];
                match Result::<(), CallError<Interrupt>>::decode_with_depth_limit(
                    MAX_SUBSCRIPTION_DECODE_DEPTH,
                    &mut input,
                ) {
                    // `Ok(())` is the product saying it is done, which ends
                    // the stream without a failure.
                    Ok(Ok(())) if input.is_empty() => Poll::Ready(None),
                    Ok(Err(interrupt)) if input.is_empty() => Poll::Ready(Some(Err(interrupt))),
                    Ok(_) | Err(_) => {
                        Poll::Ready(Some(Err(CallError::MalformedFrame {
                            reason: "host-initiated subscription interrupt did not decode"
                                .to_string(),
                        })))
                    }
                }
            }
            Poll::Ready(Some(HostInitiatedFrame::Unsupported)) => {
                self.terminated = true;
                Poll::Ready(Some(Err(CallError::Unsupported)))
            }
            Poll::Ready(Some(HostInitiatedFrame::UnknownProtocolError)) => {
                self.terminated = true;
                Poll::Ready(Some(Err(CallError::HostFailure {
                    reason: "product reported a protocol error this build cannot read".to_string(),
                })))
            }
            // The sender is gone: the host closed the manager or disposed the
            // core. That is cancellation, not a product failure.
            Poll::Ready(None) => {
                self.terminated = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<Item, Interrupt> Drop for HostInitiatedSubscription<Item, Interrupt> {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::frame::{MESSAGE_TYPE_RESPONSE, PROTOCOL_ERROR_METHOD_ID, PROTOCOL_ERROR_TRAIT_ID};
    use futures::FutureExt;
    use futures::stream;
    use parity_scale_codec::Encode;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};
    use truapi::v01;

    /// Transport that records every frame and notifies waiters when it
    /// reaches a target count. Used to wait for the subscription's
    /// background thread to drain a known number of frames.
    struct RecordingTransport {
        sent: Mutex<Vec<ProtocolMessage>>,
        cvar: std::sync::Condvar,
    }

    impl RecordingTransport {
        fn new() -> Self {
            Self {
                sent: Mutex::new(Vec::new()),
                cvar: std::sync::Condvar::new(),
            }
        }
        fn sent(&self) -> Vec<ProtocolMessage> {
            self.sent.lock().unwrap().clone()
        }
        /// Wait until at least `count` frames have been recorded, or
        /// `timeout` elapses. Returns the number of frames recorded at
        /// wake-up time.
        fn wait_for(&self, count: usize, timeout: std::time::Duration) -> usize {
            let mut guard = self.sent.lock().unwrap();
            let deadline = std::time::Instant::now() + timeout;
            while guard.len() < count {
                let now = std::time::Instant::now();
                if now >= deadline {
                    break;
                }
                let (new_guard, _) = self.cvar.wait_timeout(guard, deadline - now).unwrap();
                guard = new_guard;
            }
            guard.len()
        }
    }

    impl Transport for RecordingTransport {
        fn send(&self, message: ProtocolMessage) {
            self.sent.lock().unwrap().push(message);
            self.cvar.notify_all();
        }
        fn on_message(
            &self,
            _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
        ) -> Box<dyn FnOnce()> {
            Box::new(|| {})
        }
    }

    fn dummy_stream(items: Vec<Vec<u8>>) -> SubscriptionStream {
        Box::pin(stream::iter(
            items.into_iter().map(SubscriptionOutput::Item),
        ))
    }

    fn host_ids() -> MethodIds {
        MethodIds {
            trait_id: 195,
            method_id: 14,
        }
    }

    /// Product frame on [`host_ids`]'s address, tagged `message_type`
    /// (`Start`=0, `Receive`=1, `Interrupt`=2, `Stop`=3) with `inner` as that
    /// leg's own payload.
    fn host_frame(request_id: &str, message_type: u8, inner: Vec<u8>) -> ProtocolMessage {
        ProtocolMessage {
            request_id: request_id.into(),
            payload: Payload {
                trait_id: host_ids().trait_id,
                method_id: host_ids().method_id,
                message_type,
                value: inner,
            },
        }
    }

    #[test]
    fn host_initiated_subscription_routes_items_and_sends_stop_on_drop() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut subscription =
            manager.start::<u32, v01::GenericError>(host_ids(), vec![0xaa], transport);

        assert_eq!(transport_typed.sent()[0].request_id, "h:1");
        assert_eq!(transport_typed.sent()[0].payload.trait_id, 195);
        assert_eq!(transport_typed.sent()[0].payload.method_id, 14);
        assert_eq!(transport_typed.sent()[0].payload.value, vec![0xaa]);

        assert!(
            manager
                .handle_message(host_frame("h:1", 1, 7_u32.encode()))
                .is_none()
        );
        assert_eq!(
            futures::executor::block_on(subscription.next()),
            Some(Ok(7))
        );

        drop(subscription);
        let frames = transport_typed.sent();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].request_id, "h:1");
        assert_eq!(frames[1].payload.trait_id, 195);
        assert_eq!(frames[1].payload.method_id, 14);
        assert_eq!(frames[1].payload.message_type, MESSAGE_TYPE_STOP);
        assert_eq!(frames[1].payload.value, Vec::<u8>::new());
    }

    #[test]
    fn a_deeply_nested_host_item_is_refused_rather_than_exhausting_the_stack() {
        // A recursive product-supplied payload decodes until the thread's stack
        // is gone, and a stack overflow aborts the process rather than failing
        // the call -- no `catch_unwind` and no `panic = "abort"` handling
        // applies to it. The depth bound turns that into an ordinary refusal
        // that ends this subscription alone.
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut nested =
            manager.start::<NestedItem, v01::GenericError>(host_ids(), vec![], transport.clone());
        let mut healthy =
            manager.start::<NestedItem, v01::GenericError>(host_ids(), vec![], transport);

        // One `Deeper` byte per level, terminated by `Leaf`.
        let mut bomb = vec![0x01; (MAX_SUBSCRIPTION_DECODE_DEPTH as usize) * 4];
        bomb.push(0x00);
        manager.handle_message(host_frame("h:1", 1, bomb));
        assert!(matches!(
            futures::executor::block_on(nested.next()),
            Some(Err(_))
        ));

        // A payload inside the bound still arrives, on its own subscription.
        manager.handle_message(host_frame("h:2", 1, NestedItem::Leaf.encode()));
        assert_eq!(
            futures::executor::block_on(healthy.next()),
            Some(Ok(NestedItem::Leaf))
        );
    }

    #[test]
    fn the_depth_bound_lands_where_the_real_render_item_nests() {
        // The fixture above recurses through `Box`, which uses a different
        // `Decode` impl than the `Vec<Self>` the production type recurses
        // through. Pin the boundary on the type actually decoded here.
        fn nested(depth: u32) -> truapi::versioned::renderer::ProductRendererRenderItem {
            let mut node = truapi::v01::RendererNode::Nil;
            for _ in 0..depth {
                node = truapi::v01::RendererNode::Box {
                    modifiers: Vec::new(),
                    props: truapi::v01::BoxProps {
                        content_alignment: None,
                    },
                    children: vec![node],
                };
            }
            truapi::versioned::renderer::ProductRendererRenderItem::V1(node)
        }

        let decode = |depth: u32| {
            let bytes = nested(depth).encode();
            let mut input = &bytes[..];
            truapi::versioned::renderer::ProductRendererRenderItem::decode_with_depth_limit(
                MAX_SUBSCRIPTION_DECODE_DEPTH,
                &mut input,
            )
            .is_ok()
        };

        assert!(
            decode(MAX_SUBSCRIPTION_DECODE_DEPTH),
            "the limit must be usable"
        );
        assert!(
            !decode(MAX_SUBSCRIPTION_DECODE_DEPTH + 1),
            "one past the limit must be refused"
        );
    }

    /// Stands in for the recursive protocol payloads a product can supply.
    #[derive(Debug, PartialEq, Eq, Encode, Decode)]
    enum NestedItem {
        Leaf,
        Deeper(Box<NestedItem>),
    }

    #[test]
    fn malformed_host_item_ends_only_its_render_instance() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut malformed =
            manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport.clone());
        let mut healthy = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

        manager.handle_message(host_frame("h:1", 1, vec![0xff]));
        manager.handle_message(host_frame("h:2", 1, 9_u32.encode()));

        // A partial tree left on screen as final is the failure this prevents.
        assert_eq!(
            futures::executor::block_on(malformed.next()),
            Some(Err(CallError::MalformedFrame {
                reason: "host-initiated subscription item did not decode".to_string(),
            }))
        );
        assert_eq!(futures::executor::block_on(malformed.next()), None);
        assert_eq!(futures::executor::block_on(healthy.next()), Some(Ok(9)));
        assert_eq!(transport_typed.sent()[2].request_id, "h:1");
        assert_eq!(transport_typed.sent()[2].payload.trait_id, 195);
        assert_eq!(transport_typed.sent()[2].payload.method_id, 14);
    }

    #[test]
    fn product_interrupt_ends_one_host_render_without_echoing_stop() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut declined = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

        // A declining product sends `Interrupt(Err(error))`. An
        // `Interrupt(Ok(()))` is a clean completion instead, covered by
        // `a_clean_host_interrupt_completes_the_stream_instead_of_erroring`.
        let interrupt = truapi::CallError::<v01::GenericError>::HostFailure {
            reason: "unavailable".to_string(),
        };
        let declining = Err::<(), _>(interrupt.clone()).encode();
        manager.handle_message(host_frame("h:1", MESSAGE_TYPE_INTERRUPT, declining));

        // The product's own reason reaches the host, rather than a canned one.
        assert_eq!(
            futures::executor::block_on(declined.next()),
            Some(Err(interrupt))
        );
        assert_eq!(futures::executor::block_on(declined.next()), None);
        assert_eq!(transport_typed.sent().len(), 1);
    }

    /// An interrupt payload this build cannot read must settle the stream
    /// with an error. Reading it as a clean end would tell the product its
    /// render finished, on a frame that never said so.
    #[test]
    fn an_unreadable_host_interrupt_ends_the_stream_with_a_malformed_frame() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut undecodable =
            manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport.clone());
        let mut trailing = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

        // A `Result` discriminant this build does not know, and a clean end
        // that does not stop where its payload does.
        manager.handle_message(host_frame("h:1", MESSAGE_TYPE_INTERRUPT, vec![0xff]));
        manager.handle_message(host_frame(
            "h:2",
            MESSAGE_TYPE_INTERRUPT,
            [encode_clean_interrupt(), vec![0xff]].concat(),
        ));

        let malformed = Some(Err(CallError::MalformedFrame {
            reason: "host-initiated subscription interrupt did not decode".to_string(),
        }));
        assert_eq!(futures::executor::block_on(undecodable.next()), malformed);
        assert_eq!(futures::executor::block_on(undecodable.next()), None);
        assert_eq!(futures::executor::block_on(trailing.next()), malformed);
        assert_eq!(futures::executor::block_on(trailing.next()), None);
        // Only the two Start frames: a settled stream does not echo Stop.
        assert_eq!(transport_typed.sent().len(), 2);
    }

    #[test]
    fn protocol_error_ends_one_host_render_without_echoing_stop() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut unsupported =
            manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

        manager.handle_message(ProtocolMessage {
            request_id: "h:1".into(),
            payload: Payload {
                trait_id: PROTOCOL_ERROR_TRAIT_ID,
                method_id: PROTOCOL_ERROR_METHOD_ID,
                message_type: MESSAGE_TYPE_RESPONSE,
                value: VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
                    trait_id: host_ids().trait_id,
                    method_id: host_ids().method_id,
                })
                .encode(),
            },
        });

        assert_eq!(
            unsupported.next().now_or_never(),
            Some(Some(Err(CallError::Unsupported)))
        );
        assert_eq!(unsupported.next().now_or_never(), Some(None));
        assert_eq!(transport_typed.sent().len(), 1);
    }

    #[test]
    fn an_unreadable_protocol_error_settles_the_host_render() {
        // `(255, 255)` is correlated to this request id, so a payload from a
        // later build is still this render's terminal even though its shape
        // cannot be attributed to a pair. Leaving the slot alive instead would
        // strand the stream on a peer that has already answered.
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut render = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

        manager.handle_message(ProtocolMessage {
            request_id: "h:1".into(),
            payload: Payload {
                trait_id: PROTOCOL_ERROR_TRAIT_ID,
                method_id: PROTOCOL_ERROR_METHOD_ID,
                message_type: MESSAGE_TYPE_RESPONSE,
                // A protocol-error version this build does not know.
                value: vec![1],
            },
        });

        assert_eq!(
            render.next().now_or_never(),
            Some(Some(Err(CallError::HostFailure {
                reason: "product reported a protocol error this build cannot read".to_string(),
            }))),
            "an unreadable protocol error must settle the stream, not leave it pending"
        );
        assert_eq!(render.next().now_or_never(), Some(None));
        // Only the Start frame: a settled render does not echo Stop.
        assert_eq!(transport_typed.sent().len(), 1);
    }

    #[test]
    fn a_clean_host_interrupt_completes_the_stream_instead_of_erroring() {
        // `Interrupt` carries `Result<(), CallError<E>>`, so `Ok(())` (a
        // single `0` byte) is a clean completion. Reporting it as an error
        // would make every well-behaved product look like it had failed.
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut render = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

        manager.handle_message(host_frame(
            "h:1",
            MESSAGE_TYPE_INTERRUPT,
            encode_clean_interrupt(),
        ));

        assert_eq!(
            render.next().now_or_never(),
            Some(None),
            "a clean interrupt must end the stream without an error"
        );
        assert_eq!(transport_typed.sent().len(), 1);
    }

    #[test]
    fn unrelated_protocol_errors_do_not_end_a_host_render() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut render = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

        for value in [
            // A different (trait, method) pair than this render's own.
            VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
                trait_id: host_ids().trait_id,
                method_id: host_ids().method_id + 1,
            })
            .encode(),
            vec![0, 0],
        ] {
            manager.handle_message(ProtocolMessage {
                request_id: "h:1".into(),
                payload: Payload {
                    trait_id: PROTOCOL_ERROR_TRAIT_ID,
                    method_id: PROTOCOL_ERROR_METHOD_ID,
                    message_type: MESSAGE_TYPE_RESPONSE,
                    value,
                },
            });
        }
        assert_eq!(render.next().now_or_never(), None);

        manager.handle_message(host_frame("h:1", 1, 7_u32.encode()));
        assert_eq!(futures::executor::block_on(render.next()), Some(Ok(7)));
    }

    #[test]
    fn host_cancellation_ends_the_stream_without_reporting_an_error() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut render = manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);

        manager.close();

        assert_eq!(futures::executor::block_on(render.next()), None);
    }

    #[test]
    fn closing_host_subscriptions_ends_streams_without_sending_stop() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport: Arc<dyn Transport> = transport_typed.clone();
        let manager = HostInitiatedSubscriptionManager::new();
        let mut render =
            manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport.clone());

        manager.close();

        assert_eq!(futures::executor::block_on(render.next()), None);
        assert_eq!(transport_typed.sent().len(), 1);

        // A start against a closed manager can never be served, so it
        // interrupts rather than completing as if it had run.
        let mut after_close =
            manager.start::<u32, v01::GenericError>(host_ids(), vec![], transport);
        assert!(matches!(
            futures::executor::block_on(after_close.next()),
            Some(Err(CallError::HostFailure { .. }))
        ));
        assert_eq!(transport_typed.sent().len(), 1);
    }

    #[test]
    fn host_subscription_ids_are_isolated_by_connection_manager() {
        let first_transport_typed = Arc::new(RecordingTransport::new());
        let first_transport: Arc<dyn Transport> = first_transport_typed.clone();
        let first = HostInitiatedSubscriptionManager::new();
        let mut first_render =
            first.start::<u32, v01::GenericError>(host_ids(), vec![], first_transport);

        let second_transport_typed = Arc::new(RecordingTransport::new());
        let second_transport: Arc<dyn Transport> = second_transport_typed.clone();
        let second = HostInitiatedSubscriptionManager::new();
        let mut second_render =
            second.start::<u32, v01::GenericError>(host_ids(), vec![], second_transport);

        assert_eq!(first_transport_typed.sent()[0].request_id, "h:1");
        assert_eq!(second_transport_typed.sent()[0].request_id, "h:1");

        first.handle_message(host_frame("h:1", 1, 7_u32.encode()));
        second.handle_message(host_frame("h:1", 1, 9_u32.encode()));

        assert_eq!(
            futures::executor::block_on(first_render.next()),
            Some(Ok(7))
        );
        assert_eq!(
            futures::executor::block_on(second_render.next()),
            Some(Ok(9))
        );
    }

    struct PendingDropStream {
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Drop for PendingDropStream {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    impl futures::Stream for PendingDropStream {
        type Item = SubscriptionOutput;

        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            Poll::Pending
        }
    }

    /// Register a never-ending stream then immediately stop it. The
    /// stream's first poll must observe cancellation and exit without
    /// having pushed any frame.
    #[test]
    fn register_then_stop_emits_no_extra_frames() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());
        let slow_stream: SubscriptionStream = Box::pin(stream::pending());
        manager.register("p:1".to_string(), 7, 99, slow_stream, transport_dyn);
        manager.handle_stop("p:1");
        // Give the worker thread a beat to observe the cancel.
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            transport_typed.sent().is_empty(),
            "stopped subscription must not push any frame"
        );
    }

    /// A host stream that fails after its first item must deliver that item,
    /// then the interrupt carrying the failure, and stop there: the values
    /// behind the failure never reach the peer, and no clean terminator
    /// follows it.
    #[test]
    fn a_mid_stream_interrupt_ends_the_subscription_where_it_happens() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        // Drive the worker on the caller's thread so the frame list is
        // complete by the time `register` returns: the assertion below is
        // that nothing follows the interrupt.
        let inline_spawner: Spawner = Arc::new(futures::executor::block_on);
        let manager = SubscriptionManager::new(inline_spawner);
        let failure: CallError<v01::GenericError> = CallError::HostFailure {
            reason: "platform stream failed".to_string(),
        };
        let items = subscription_stream(stream::iter(vec![
            Ok(1_u32),
            Err(failure.clone()),
            Ok(2_u32),
        ]));
        manager.register("p:1".to_string(), 7, 99, items, transport_dyn);

        let frames = transport_typed.sent();
        // The item behind the interrupt is dropped, and a stream that ended
        // with one does not also report a clean end.
        assert_eq!(frames.len(), 2, "expected 1 receive frame + 1 interrupt");
        assert_eq!(frames[0].payload.message_type, MESSAGE_TYPE_RECEIVE);
        assert_eq!(frames[0].payload.value, 1_u32.encode());
        assert_eq!(frames[1].payload.message_type, MESSAGE_TYPE_INTERRUPT);
        assert_eq!(frames[1].payload.value, subscription_interrupt(failure));
    }

    /// A stream that yields 2 items then ends naturally must produce 2
    /// `_receive` frames followed by one `_interrupt` frame.
    #[test]
    fn register_completion_emits_interrupt() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());
        let items = dummy_stream(vec![vec![0xaa], vec![0xbb]]);
        manager.register("p:1".to_string(), 7, 99, items, transport_dyn);
        let observed = transport_typed.wait_for(3, std::time::Duration::from_secs(2));
        assert_eq!(observed, 3, "expected 2 receive frames + 1 interrupt");
        let frames = transport_typed.sent();
        assert_eq!(frames[0].payload.trait_id, 7);
        assert_eq!(frames[0].payload.method_id, 99);
        assert_eq!(frames[0].payload.message_type, MESSAGE_TYPE_RECEIVE);
        assert_eq!(frames[0].payload.value, vec![0xaa]);
        assert_eq!(frames[1].payload.method_id, 99);
        assert_eq!(frames[1].payload.message_type, MESSAGE_TYPE_RECEIVE);
        assert_eq!(frames[1].payload.value, vec![0xbb]);
        assert_eq!(frames[2].payload.trait_id, 7);
        assert_eq!(frames[2].payload.method_id, 99);
        assert_eq!(frames[2].payload.message_type, MESSAGE_TYPE_INTERRUPT);
        assert_eq!(frames[2].payload.value, encode_clean_interrupt());
    }

    /// Calling `handle_stop` twice on the same request id must be a
    /// no-op the second time around (the entry has already been removed,
    /// no panic, no extra frames).
    #[test]
    fn double_stop_is_idempotent() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());
        let slow_stream: SubscriptionStream = Box::pin(stream::pending());
        manager.register("p:1".to_string(), 7, 99, slow_stream, transport_dyn);
        manager.handle_stop("p:1");
        // Second call must not panic and must not emit any frame.
        manager.handle_stop("p:1");
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            transport_typed.sent().is_empty(),
            "double-stop must not emit any frame"
        );
    }

    /// The manager must drive subscriptions through the injected spawner,
    /// not by reaching out to `std::thread::spawn` itself. The counter
    /// inside the test spawner is the proof.
    #[test]
    fn subscription_uses_provided_spawner_not_native_thread() {
        let invocations = Arc::new(AtomicUsize::new(0));
        let invocations_for_spawner = invocations.clone();
        let spawner: Spawner = Arc::new(move |fut: BoxFuture<'static, ()>| {
            invocations_for_spawner.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || futures::executor::block_on(fut));
        });

        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(spawner);
        let items = dummy_stream(vec![vec![0xcc]]);
        manager.register("p:1".to_string(), 7, 99, items, transport_dyn);

        // Wait for the worker future to drain to completion so we know
        // the spawner closure ran on this path.
        let _ = transport_typed.wait_for(2, std::time::Duration::from_secs(2));
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            1,
            "spawner must be invoked exactly once per register",
        );
    }

    /// A `_stop` arriving before `activate` (the stop-before-register race on
    /// non-serialized transports) must abort the subscription: no `_receive`
    /// frames are emitted even though the stream had items to yield.
    #[test]
    fn stop_before_activate_aborts_subscription() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());
        let token = manager.reserve("p:1".to_string());
        manager.handle_stop("p:1");
        let items = dummy_stream(vec![vec![0x01], vec![0x02]]);
        manager.activate(token, 7, 99, items, transport_dyn);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            transport_typed.sent().is_empty(),
            "a stop before activate must abort the subscription"
        );
    }

    /// Re-using a live request id (the duplicate-`_start` case) supersedes the
    /// previous subscription rather than leaking it: the first stream is
    /// stopped, only the second runs, and the superseded stream leaves no
    /// frames behind.
    #[test]
    fn duplicate_start_supersedes_previous_without_leak() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());

        // First subscription never yields; the second reservation for the
        // same id must stop it.
        let pending: SubscriptionStream = Box::pin(stream::pending());
        manager.register("p:1".to_string(), 7, 99, pending, transport_dyn.clone());

        // Second subscription yields one item then ends.
        let items = dummy_stream(vec![vec![0xaa]]);
        manager.register("p:1".to_string(), 7, 99, items, transport_dyn);

        // Exactly the second stream's frames appear: one receive + one
        // completion interrupt. The first (pending) stream contributes none.
        let observed = transport_typed.wait_for(2, std::time::Duration::from_secs(2));
        assert_eq!(
            observed, 2,
            "expected the second stream's receive + interrupt only"
        );
        let frames = transport_typed.sent();
        assert_eq!(frames[0].payload.trait_id, 7);
        assert_eq!(frames[0].payload.method_id, 99);
        assert_eq!(frames[0].payload.value, vec![0xaa]);
        assert_eq!(frames[1].payload.trait_id, 7);
        assert_eq!(frames[1].payload.method_id, 99);

        manager.handle_stop("p:1");
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            transport_typed.sent().len(),
            2,
            "no leaked frames from the superseded stream"
        );
    }

    #[test]
    fn cancel_all_stops_live_subscription_streams() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stream: SubscriptionStream = Box::pin(PendingDropStream {
            dropped: dropped.clone(),
        });

        manager.register("p:1".to_string(), 7, 99, stream, transport_dyn);
        manager.cancel_all();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !dropped.load(Ordering::SeqCst) {
            assert!(
                std::time::Instant::now() < deadline,
                "cancel_all did not drop the live subscription stream"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            transport_typed.sent().is_empty(),
            "runtime disposal cancellation must not emit an interrupt frame"
        );
    }
}
