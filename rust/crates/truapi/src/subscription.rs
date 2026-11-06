//! Subscription lifecycle management.
//!
//! Tracks active subscriptions (start/receive/stop/interrupt) and handles
//! cleanup when either side terminates. Each registered subscription drives
//! its stream on a caller-supplied [`Spawner`]; the manager itself never
//! creates threads or runtimes.

use std::{
	collections::HashMap,
	marker::PhantomData,
	pin::Pin,
	sync::{
		Arc, Mutex,
		atomic::{AtomicU64, Ordering},
	},
	task::{Context, Poll},
};

use futures::{
	Stream, StreamExt,
	channel::mpsc,
	future::{BoxFuture, Either, select},
	stream::BoxStream,
};
use parity_scale_codec::{Decode, DecodeLimit, Encode};
use truapi::CallError;

use crate::{
	frame::{
		IdFactory, MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_RESPONSE,
		MESSAGE_TYPE_START, MESSAGE_TYPE_STOP, PROTOCOL_ERROR_KEY, Payload, ProtocolErrorV1,
		ProtocolMessage, VersionedProtocolError, encode_clean_interrupt,
	},
	generated::wire_table::MethodIds,
	protocol_error::decode_protocol_error_payload,
	transport::Transport,
};

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
	Box::pin(stream.map(|item| match item {
		Ok(item) => SubscriptionOutput::Item(item.encode()),
		Err(interrupt) => SubscriptionOutput::Interrupt(subscription_interrupt(interrupt)),
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
		if let Some(Slot::Live { cancel, .. }) =
			active.insert(request_id.clone(), Slot::Pending { generation, cancelled: false })
		{
			cancel();
		}
		ReservationToken { request_id, generation }
	}

	/// Whether `request_id` holds a slot, reserved or live.
	#[cfg(test)]
	pub fn holds(&self, request_id: &str) -> bool {
		self.active.lock().unwrap().contains_key(request_id)
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
		let ReservationToken { request_id, generation } = token;
		let rid = request_id.clone();
		let stream_transport = transport.clone();

		// Cancellation channel.
		let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel::<()>();

		// Transition the reserved slot to live, unless a `_stop` cancelled it
		// or a newer reservation superseded it while the handler resolved.
		{
			let mut active = self.active.lock().unwrap();
			match active.get_mut(&request_id) {
				Some(Slot::Pending { generation: g, cancelled }) if *g == generation => {
					if *cancelled {
						active.remove(&request_id);
						return;
					}
				},
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
								},
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
								},
								None => break true,
							}
						},
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
			},
			Some(Slot::Live { .. }) => {
				if let Some(Slot::Live { cancel, .. }) = active.remove(request_id) {
					cancel();
				}
			},
			None => {},
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
				.filter_map(|(_, slot)| match slot {
					Slot::Pending { .. } => None,
					Slot::Live { cancel, .. } => Some(cancel),
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
	/// The product connection closed while the stream was open. A failure
	/// rather than a finish: the product never ended the stream, so the host
	/// may open it again on the next connection.
	ConnectionClosed,
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

	/// Await the first subscription result and release the stream.
	/// Polling starts the call; dropping it stops the subscription.
	pub async fn request<Item, Interrupt>(
		&self,
		ids: MethodIds,
		payload: Vec<u8>,
		transport: Arc<dyn Transport>,
	) -> Result<Item, CallError<Interrupt>>
	where
		Item: Decode + Send + Unpin + 'static,
		Interrupt: Decode + Send + Unpin + 'static,
	{
		self.start(ids, payload, transport).next().await.unwrap_or_else(|| {
			Err(CallError::HostFailure { reason: "product closed without a response".into() })
		})
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
			let mut state = self.state.lock().expect("host subscription state mutex poisoned");
			if state.closed {
				return truapi::Subscription::interrupted(CallError::HostFailure {
					reason: "host-initiated subscriptions are closed".to_string(),
				});
			}
			let request_id = state.ids.next_id();
			state.active.insert(request_id.clone(), HostInitiatedSlot { ids, sender });
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

		let mut state = self.state.lock().expect("host subscription state mutex poisoned");
		let slot = state.active.get(&message.request_id)?;
		let key = (message.payload.trait_id, message.payload.method_id);
		// The protocol-error check MUST precede the trait guard. A protocol
		// error is addressed to the reserved trait, never to this slot's, so
		// guarding on the trait first would make the arm below dead code and
		// silently drop the frame that reports our start as unsupported.
		if key == PROTOCOL_ERROR_KEY {
			if message.payload.message_type != MESSAGE_TYPE_RESPONSE {
				return None;
			}
			let frame = match decode_protocol_error_payload(&message.payload.value) {
				Ok(Some(VersionedProtocolError::V1(
					ProtocolErrorV1::UnsupportedMessage { trait_id, method_id } |
					ProtocolErrorV1::UnsupportedVersion { trait_id, method_id, .. },
				))) => {
					// Only OUR start frame going unsupported ends this render;
					// an error about any other pair belongs to a different
					// subscription. A product refusing the start's version ends
					// it the same way.
					if (trait_id, method_id) != (slot.ids.trait_id, slot.ids.method_id) {
						return None;
					}
					HostInitiatedFrame::Unsupported
				},
				// A protocol error from a later build. Its shape is unknowable
				// here, so it cannot be attributed to a pair, but it is
				// correlated to this request id and the peer will not answer
				// again. Settling is what stops the stream waiting forever.
				Ok(None) => HostInitiatedFrame::UnknownProtocolError,
				// Unreachable in practice: `ProtocolMessage::decode` rejects a
				// malformed protocol-error payload at the frame boundary.
				Err(_) => return None,
			};
			let sender = state.active.remove(&message.request_id)?.sender;
			drop(state);
			let _ = sender.unbounded_send(frame);
			return None;
		}
		if message.payload.trait_id != slot.ids.trait_id ||
			message.payload.method_id != slot.ids.method_id
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
			let sender = state.active.remove(&message.request_id)?.sender;
			drop(state);
			let _ = sender.unbounded_send(HostInitiatedFrame::Interrupt(message.payload.value));
		}
		None
	}

	/// Fail every active host-initiated stream because its product connection
	/// closed, without sending new frames. A stream that ended cleanly would
	/// read to the host as a body the product finished, so it would never be
	/// opened again on the next connection.
	pub fn close(&self) {
		let active = {
			let mut state = self.state.lock().expect("host subscription state mutex poisoned");
			state.closed = true;
			core::mem::take(&mut state.active)
		};
		for (_, slot) in active {
			let _ = slot.sender.unbounded_send(HostInitiatedFrame::ConnectionClosed);
		}
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
			.remove(&self.request_id);
		if removed.is_some() {
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
					},
				}
			},
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
					Ok(_) | Err(_) => Poll::Ready(Some(Err(CallError::MalformedFrame {
						reason: "host-initiated subscription interrupt did not decode".to_string(),
					}))),
				}
			},
			Poll::Ready(Some(HostInitiatedFrame::Unsupported)) => {
				self.terminated = true;
				Poll::Ready(Some(Err(CallError::Unsupported)))
			},
			Poll::Ready(Some(HostInitiatedFrame::UnknownProtocolError)) => {
				self.terminated = true;
				Poll::Ready(Some(Err(CallError::HostFailure {
					reason: "product reported a protocol error this build cannot read".to_string(),
				})))
			},
			Poll::Ready(Some(HostInitiatedFrame::ConnectionClosed)) => {
				self.terminated = true;
				Poll::Ready(Some(Err(CallError::HostFailure {
					reason: "the product connection closed while the stream was open".to_string(),
				})))
			},
			// The sender is gone without a terminal frame: the manager was
			// dropped with the core. That is cancellation, not a product failure.
			Poll::Ready(None) => {
				self.terminated = true;
				Poll::Ready(None)
			},
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
mod tests;
