//! `subxt-rpcs` client adapter for host-provided JSON-RPC pipes.
//!
//! The platform owns the physical chain connection. This module owns only the
//! generic JSON-RPC mechanics needed to expose that pipe as a
//! [`subxt_rpcs::RpcClientT`]: request correlation, subscription routing, and
//! best-effort unsubscribe on subscription drop.

use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::platform::JsonRpcConnection;
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, pin_mut};
use futures::{Stream, StreamExt};
use serde::{Serialize, Serializer};
use serde_json::value::RawValue;
use subxt_rpcs::client::{RawRpcFuture, RawRpcSubscription, RpcClientT};
use subxt_rpcs::{Error as RpcError, UserError};
use tracing::instrument;

use crate::subscription::Spawner;

const MAX_BUFFERED_SUBSCRIPTIONS: usize = 64;
const MAX_BUFFERED_ITEMS_PER_SUBSCRIPTION: usize = 256;
const DEFAULT_REQUEST_ID_PREFIX: &str = "truapi:";

/// JSON-RPC client backed by a host-owned [`JsonRpcConnection`].
pub struct HostRpcClient {
    inner: Arc<HostRpcClientInner>,
}

struct HostRpcClientInner {
    connection: Arc<dyn JsonRpcConnection>,
    request_id_prefix: &'static str,
    request_ids: AtomicU64,
    user_handles: AtomicUsize,
    closed: AtomicBool,
    stop_response_loop: Mutex<Option<oneshot::Sender<()>>>,
    pending: Mutex<HashMap<String, PendingRequest>>,
    subscriptions: Mutex<SubscriptionState>,
}

/// Where a subscription's notifications are delivered.
type SubscriptionSink = mpsc::UnboundedSender<Result<Box<RawValue>, RpcError>>;

/// One subscription id, in the only two states it can be in.
///
/// A notification can arrive before the task that issued the subscribe call has
/// published its sink, so an id is born `Pending` and holds its own notifications
/// until activation drains them.
enum SubscriptionEntry {
    /// Notifications received before a sink existed, oldest first.
    Pending(Vec<Box<RawValue>>),
    /// The sink the subscriber reads.
    Active(SubscriptionSink),
}

/// Every subscription this client routes, under one lock.
///
/// Both states live in one map so that activation, delivery, unsubscribe and
/// connection close are each a single critical section. Ordering then follows
/// from the lock alone: there is no second lock to acquire in the right order,
/// and no window between publishing a sink and replaying what preceded it.
#[derive(Default)]
struct SubscriptionState {
    entries: HashMap<String, SubscriptionEntry>,
}

/// Take the subscription lock, tolerating a poisoned mutex.
///
/// `close_with_error` and `unsubscribe` both run from `Drop`. A panic elsewhere
/// that poisoned this lock would make an `unwrap` here panic a second time
/// during unwinding, which aborts the process instead of reporting the original
/// failure. The state behind the lock is a plain map, so recovering it is safe.
fn lock_subscriptions(
    lock: &Mutex<SubscriptionState>,
) -> std::sync::MutexGuard<'_, SubscriptionState> {
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl SubscriptionState {
    /// Publish `tx` for `subscription_id` and replay whatever arrived before it,
    /// oldest first.
    ///
    /// The replay happens here, under the caller's guard, rather than being
    /// handed back to be replayed afterwards. That is the whole ordering
    /// guarantee: a buffer that never escapes the lock cannot be replayed after
    /// it is released, so no later notification can overtake it. Returning the
    /// items instead would leave the invariant to a comment, which is what the
    /// two-lock version did.
    ///
    /// Sending under the lock here is safe, where it is not in
    /// [`Self::deliver_or_buffer`], and the difference is the receiver rather
    /// than the send. `subscribe` creates the channel and calls this before it
    /// builds the `SubscriptionStream` that owns the receiving half, so nothing
    /// is polling it yet and there is no waker to run. A send on the delivery
    /// path reaches a receiver the subscriber is already polling, which is what
    /// makes the waker reachable there.
    fn activate(&mut self, subscription_id: String, tx: SubscriptionSink) {
        let previous = self
            .entries
            .insert(subscription_id, SubscriptionEntry::Active(tx.clone()));
        // Re-activating an id that is already active replaces the sink and has
        // nothing buffered, which is also the never-seen case.
        if let Some(SubscriptionEntry::Pending(items)) = previous {
            for item in items {
                let _ = tx.unbounded_send(Ok(item));
            }
        }
    }

    /// Route one notification: hand it back for delivery when the subscription
    /// is active, buffer it when it is not.
    ///
    /// An active subscription's sink is returned rather than sent to here, so
    /// the caller sends with the lock released. `unbounded_send` wakes the
    /// receiving task, and a waker is embedder code: an executor that polls
    /// inline would re-enter this module, and `unsubscribe` on the resulting
    /// stream drop takes this same non-reentrant lock. [`Self::activate`] sends
    /// under the lock because its receiver has no waker yet; see there.
    ///
    /// Ordering survives the release because the only thing this races is
    /// `activate`, which replays under the same lock, so seeing `Active` at all
    /// means the replay is already done. It does not order against
    /// `close_with_error`, which can drain and report on another thread between
    /// the sink being taken and the send: a subscriber can see one item behind
    /// the close error. That window is the same one the two-lock version had.
    ///
    /// Two deliveries for one subscription stay in order because there is a
    /// single response loop calling this, not because of the lock. That is a
    /// property of the call graph: `handle_notification` has one caller,
    /// `handle_frame`, which runs only on the response-loop task.
    fn deliver_or_buffer(
        &mut self,
        subscription_id: String,
        item: Box<RawValue>,
    ) -> Option<(SubscriptionSink, Box<RawValue>)> {
        match self.entries.get_mut(&subscription_id) {
            Some(SubscriptionEntry::Active(tx)) => {
                return Some((tx.clone(), item));
            }
            Some(SubscriptionEntry::Pending(items)) => {
                if items.len() < MAX_BUFFERED_ITEMS_PER_SUBSCRIPTION {
                    items.push(item);
                }
            }
            // The cap counts subscriptions holding a buffer, not active ones:
            // an active subscription costs nothing to remember here.
            None => {
                if self.pending_count() < MAX_BUFFERED_SUBSCRIPTIONS {
                    self.entries
                        .insert(subscription_id, SubscriptionEntry::Pending(vec![item]));
                }
            }
        }
        None
    }

    /// Forget a subscription, whichever state it is in.
    fn remove(&mut self, subscription_id: &str) {
        self.entries.remove(subscription_id);
    }

    /// Take every entry, leaving the state empty.
    fn drain(&mut self) -> HashMap<String, SubscriptionEntry> {
        mem::take(&mut self.entries)
    }

    fn pending_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| matches!(entry, SubscriptionEntry::Pending(_)))
            .count()
    }
}

struct HostRpcClientLease {
    inner: Arc<HostRpcClientInner>,
}

struct PendingRequest {
    tx: oneshot::Sender<Result<Box<RawValue>, RpcError>>,
}

#[derive(Debug, derive_more::Display, derive_more::Error)]
#[display("{}", _0)]
struct HostRpcClientError(#[error(not(source))] String);

#[derive(Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    id: &'a str,
    method: &'a str,
    #[serde(serialize_with = "serialize_json_rpc_params")]
    params: Option<&'a RawValue>,
}

fn serialize_json_rpc_params<S>(
    params: &Option<&RawValue>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match params {
        Some(params) => params.serialize(serializer),
        None => <[(); 0]>::default().serialize(serializer),
    }
}

impl HostRpcClient {
    /// Wrap `connection` and start the response pump on `spawner`.
    pub fn new(connection: Arc<dyn JsonRpcConnection>, spawner: Spawner) -> Self {
        Self::with_request_id_prefix(connection, spawner, DEFAULT_REQUEST_ID_PREFIX)
    }

    /// Like [`Self::new`], with every request id starting with `prefix`, so a
    /// host can recognise this client's traffic on its connection.
    pub fn with_request_id_prefix(
        connection: Arc<dyn JsonRpcConnection>,
        spawner: Spawner,
        prefix: &'static str,
    ) -> Self {
        let (stop_response_tx, stop_response_rx) = oneshot::channel();
        let client = Self {
            inner: Arc::new(HostRpcClientInner {
                connection,
                request_id_prefix: prefix,
                request_ids: AtomicU64::new(1),
                user_handles: AtomicUsize::new(1),
                closed: AtomicBool::new(false),
                stop_response_loop: Mutex::new(Some(stop_response_tx)),
                pending: Mutex::new(HashMap::new()),
                subscriptions: Mutex::new(SubscriptionState::default()),
            }),
        };
        client.spawn_response_loop(spawner, stop_response_rx);
        client
    }

    /// Whether the underlying response stream has ended or failed.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Relaxed)
    }

    /// Send a JSON-RPC request without waiting for its response.
    ///
    /// Used by best-effort notifications where the caller must not block on
    /// the remote endpoint acknowledging the request.
    pub fn send_fire_and_forget(
        &self,
        method: &str,
        params: Option<Box<RawValue>>,
    ) -> Result<(), RpcError> {
        if self.inner.closed.load(Ordering::Relaxed) {
            return Err(client_error("json-rpc connection is closed"));
        }
        let id = self.inner.next_request_id();
        self.inner.send_request(&id, method, params.as_deref())
    }

    fn spawn_response_loop(&self, spawner: Spawner, stop_rx: oneshot::Receiver<()>) {
        let inner = self.inner.clone();
        let fut = async move {
            let mut responses = inner.connection.responses();
            let stop = stop_rx.fuse();
            pin_mut!(stop);
            loop {
                futures::select! {
                    _ = stop => return,
                    frame = responses.next().fuse() => match frame {
                        Some(frame) => {
                            if let Err(error) = inner.handle_frame(&frame) {
                                inner.close_with_error(error);
                                return;
                            }
                        }
                        None => {
                            inner.close_with_error(client_error("json-rpc response stream ended"));
                            return;
                        }
                    }
                }
            }
        };
        (spawner)(fut.boxed());
    }
}

impl Clone for HostRpcClient {
    fn clone(&self) -> Self {
        self.inner.retain_user_handle();
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Drop for HostRpcClient {
    fn drop(&mut self) {
        self.inner.release_user_handle();
    }
}

impl HostRpcClientInner {
    fn retain_user_handle(&self) {
        self.user_handles.fetch_add(1, Ordering::Relaxed);
    }

    fn acquire_lease(self: &Arc<Self>) -> HostRpcClientLease {
        self.retain_user_handle();
        HostRpcClientLease {
            inner: self.clone(),
        }
    }

    fn release_user_handle(&self) {
        let previous = self.user_handles.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "host rpc client handle count underflow");
        if previous == 1 {
            self.close_with_error(client_error("json-rpc client dropped"));
        }
    }

    fn next_request_id(&self) -> String {
        format!(
            "{}{}",
            self.request_id_prefix,
            self.request_ids.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn send_request(
        &self,
        id: &str,
        method: &str,
        params: Option<&RawValue>,
    ) -> Result<(), RpcError> {
        let normalized_params = normalize_outbound_params(method, params)?;
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params: normalized_params.as_deref().or(params),
        };
        let encoded = serde_json::to_string(&request).map_err(RpcError::Serialization)?;
        self.connection.send(encoded);
        Ok(())
    }

    async fn request(
        &self,
        method: &str,
        params: Option<Box<RawValue>>,
    ) -> Result<Box<RawValue>, RpcError> {
        let id = self.next_request_id();
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if self.closed.load(Ordering::Relaxed) {
                return Err(client_error("json-rpc connection is closed"));
            }
            pending.insert(id.clone(), PendingRequest { tx });
        }

        if let Err(error) = self.send_request(&id, method, params.as_deref()) {
            self.pending.lock().unwrap().remove(&id);
            return Err(error);
        }

        rx.await
            .map_err(|_| client_error("json-rpc request was cancelled"))?
    }

    async fn subscribe(
        self: Arc<Self>,
        method: &str,
        params: Option<Box<RawValue>>,
        unsubscribe_method: &str,
        lease: HostRpcClientLease,
    ) -> Result<RawRpcSubscription, RpcError> {
        let raw_id = self.request(method, params).await?;
        let subscription_id = subscription_id_from_raw(raw_id.as_ref())?;
        let (tx, rx) = mpsc::unbounded();
        {
            // Activation and replay are one critical section, so a notification
            // arriving now waits for the lock and lands behind what it follows.
            let mut state = self.subscriptions.lock().unwrap();
            if self.closed.load(Ordering::Relaxed) {
                return Err(client_error("json-rpc connection is closed"));
            }
            state.activate(subscription_id.clone(), tx.clone());
        }

        let stream = SubscriptionStream {
            inner: rx,
            client: self,
            _lease: lease,
            subscription_id: subscription_id.clone(),
            unsubscribe_method: unsubscribe_method.to_string(),
            closed: false,
        };
        Ok(RawRpcSubscription {
            stream: Box::pin(stream),
            id: Some(subscription_id),
        })
    }

    fn unsubscribe(&self, subscription_id: &str, unsubscribe_method: &str) {
        lock_subscriptions(&self.subscriptions).remove(subscription_id);
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        let id = self.next_request_id();
        let params = RawValue::from_string(format!(
            "[{}]",
            serde_json::to_string(subscription_id).unwrap_or_else(|_| "\"\"".to_string())
        ));
        if let Ok(params) = params {
            let _ = self.send_request(&id, unsubscribe_method, Some(params.as_ref()));
        }
    }

    #[instrument(skip_all, fields(runtime.method = "host_rpc_client.handle_frame"))]
    fn handle_frame(&self, frame: &str) -> Result<(), RpcError> {
        let value: serde_json::Value =
            serde_json::from_str(frame).map_err(RpcError::Deserialization)?;

        if value.get("method").is_some() && value.get("params").is_some() {
            self.handle_notification(&value)?;
            return Ok(());
        }

        let Some(request_id) = value.get("id").and_then(json_id) else {
            return Ok(());
        };
        let Some(pending) = self.pending.lock().unwrap().remove(&request_id) else {
            return Ok(());
        };

        if let Some(result) = value.get("result") {
            let raw = raw_value_from_json(result)?;
            let _ = pending.tx.send(Ok(raw));
            return Ok(());
        }

        if let Some(error) = value.get("error") {
            let _ = pending.tx.send(Err(user_error_from_json(error)));
            return Ok(());
        }

        let _ = pending.tx.send(Err(client_error(
            "json-rpc response missing result and error",
        )));
        Ok(())
    }

    fn handle_notification(&self, value: &serde_json::Value) -> Result<(), RpcError> {
        let Some(params) = value.get("params") else {
            return Ok(());
        };
        let Some(subscription_id) = params.get("subscription").and_then(json_id) else {
            return Ok(());
        };
        let Some(result) = params.get("result") else {
            return Ok(());
        };
        let raw = raw_value_from_json(result)?;
        self.deliver_or_buffer_subscription_item(subscription_id, raw);
        Ok(())
    }

    fn deliver_or_buffer_subscription_item(&self, subscription_id: String, item: Box<RawValue>) {
        let delivery =
            lock_subscriptions(&self.subscriptions).deliver_or_buffer(subscription_id, item);
        // Outside the critical section: see `SubscriptionState::deliver_or_buffer`.
        if let Some((tx, item)) = delivery {
            let _ = tx.unbounded_send(Ok(item));
        }
    }

    fn close_with_error(&self, error: RpcError) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(stop) = self.stop_response_loop.lock().unwrap().take() {
            let _ = stop.send(());
        }
        self.connection.close();

        let pending = {
            let mut pending = self.pending.lock().unwrap();
            mem::take(&mut *pending)
        };
        for (_, pending) in pending {
            let _ = pending.tx.send(Err(client_error(format!(
                "json-rpc connection closed: {error}"
            ))));
        }

        // Taken in one step, so a close error cannot land between an activation
        // and the replay it owes. A buffer with no reader is dropped with it.
        let subscriptions = lock_subscriptions(&self.subscriptions).drain();
        for (_, entry) in subscriptions {
            if let SubscriptionEntry::Active(tx) = entry {
                let _ = tx.unbounded_send(Err(client_error(format!(
                    "json-rpc connection closed: {error}"
                ))));
            }
        }
    }
}

impl Drop for HostRpcClientLease {
    fn drop(&mut self) {
        self.inner.release_user_handle();
    }
}

impl RpcClientT for HostRpcClient {
    fn request_raw<'a>(
        &'a self,
        method: &'a str,
        params: Option<Box<RawValue>>,
    ) -> RawRpcFuture<'a, Box<RawValue>> {
        Box::pin(async move { self.inner.request(method, params).await })
    }

    fn subscribe_raw<'a>(
        &'a self,
        sub: &'a str,
        params: Option<Box<RawValue>>,
        unsub: &'a str,
    ) -> RawRpcFuture<'a, RawRpcSubscription> {
        let lease = self.inner.acquire_lease();
        Box::pin(async move {
            self.inner
                .clone()
                .subscribe(sub, params, unsub, lease)
                .await
        })
    }
}

struct SubscriptionStream {
    inner: mpsc::UnboundedReceiver<Result<Box<RawValue>, RpcError>>,
    client: Arc<HostRpcClientInner>,
    _lease: HostRpcClientLease,
    subscription_id: String,
    unsubscribe_method: String,
    closed: bool,
}

impl Drop for SubscriptionStream {
    fn drop(&mut self) {
        if !self.closed {
            self.closed = true;
            self.client
                .unsubscribe(&self.subscription_id, &self.unsubscribe_method);
        }
    }
}

impl Stream for SubscriptionStream {
    type Item = Result<Box<RawValue>, RpcError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(None) => {
                this.closed = true;
                Poll::Ready(None)
            }
            other => other,
        }
    }
}

fn raw_value_from_json(value: &serde_json::Value) -> Result<Box<RawValue>, RpcError> {
    RawValue::from_string(value.to_string()).map_err(RpcError::Deserialization)
}

/// PAPI's modern middleware requires the array variant even though Subxt emits
/// the protocol's valid single-hash unpin form.
fn normalize_outbound_params(
    method: &str,
    params: Option<&RawValue>,
) -> Result<Option<Box<RawValue>>, RpcError> {
    if method != "chainHead_v1_unpin" {
        return Ok(None);
    }
    let Some(params) = params else {
        return Ok(None);
    };
    let mut params: Vec<serde_json::Value> =
        serde_json::from_str(params.get()).map_err(RpcError::Serialization)?;
    let Some(hash_slot @ serde_json::Value::String(_)) = params.get_mut(1) else {
        return Ok(None);
    };
    let hash = mem::take(hash_slot);
    *hash_slot = serde_json::Value::Array(vec![hash]);
    let encoded = serde_json::to_string(&params).map_err(RpcError::Serialization)?;
    RawValue::from_string(encoded)
        .map(Some)
        .map_err(RpcError::Serialization)
}

fn subscription_id_from_raw(raw: &RawValue) -> Result<String, RpcError> {
    let value: serde_json::Value =
        serde_json::from_str(raw.get()).map_err(RpcError::Deserialization)?;
    json_id(&value).ok_or_else(|| client_error("json-rpc subscription id is not a string"))
}

fn json_id(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn user_error_from_json(value: &serde_json::Value) -> RpcError {
    match serde_json::from_value::<UserError>(value.clone()) {
        Ok(error) => RpcError::User(error),
        Err(error) => RpcError::Deserialization(error),
    }
}

fn client_error(reason: impl Into<String>) -> RpcError {
    RpcError::Client(Box::new(HostRpcClientError(reason.into())))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    use futures::executor::block_on;
    use futures::stream::BoxStream;
    use serde_json::{Value, json};
    use subxt_rpcs::RpcClient;
    use subxt_rpcs::client::rpc_params;

    use crate::subscription::thread_per_subscription_spawner;

    struct TrackingConnection {
        sender: Mutex<Option<mpsc::UnboundedSender<String>>>,
        receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
        sent: Mutex<Vec<Value>>,
        close_count: AtomicUsize,
    }

    impl TrackingConnection {
        fn new() -> Arc<Self> {
            let (tx, rx) = mpsc::unbounded();
            Arc::new(Self {
                sender: Mutex::new(Some(tx)),
                receiver: Mutex::new(Some(rx)),
                sent: Mutex::new(Vec::new()),
                close_count: AtomicUsize::new(0),
            })
        }

        fn close_count(&self) -> usize {
            self.close_count.load(Ordering::SeqCst)
        }

        fn sent(&self) -> Vec<Value> {
            self.sent.lock().unwrap().clone()
        }
    }

    impl JsonRpcConnection for TrackingConnection {
        fn send(&self, request: String) {
            let Ok(value) = serde_json::from_str::<Value>(&request) else {
                return;
            };
            self.sent.lock().unwrap().push(value.clone());
            let Some(id) = value.get("id").cloned() else {
                return;
            };
            if value.get("method").and_then(Value::as_str) == Some("sub") {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": "sub-1",
                });
                if let Some(sender) = self.sender.lock().unwrap().as_ref() {
                    let _ = sender.unbounded_send(response.to_string());
                }
            }
        }

        fn responses(&self) -> BoxStream<'static, String> {
            self.receiver
                .lock()
                .unwrap()
                .take()
                .expect("responses called twice")
                .boxed()
        }

        fn close(&self) {
            self.close_count.fetch_add(1, Ordering::SeqCst);
            self.sender.lock().unwrap().take();
        }
    }

    #[test]
    fn dropping_one_shot_client_closes_connection_lease() {
        let connection = TrackingConnection::new();
        let spawner: Spawner = Arc::new(|_| {});

        {
            let client = HostRpcClient::new(connection.clone(), spawner);
            client
                .send_fire_and_forget("statement_submit", None)
                .unwrap();
        }

        assert_eq!(connection.close_count(), 1);
    }

    #[test]
    fn requests_without_arguments_serialize_empty_params() {
        let connection = TrackingConnection::new();
        let spawner: Spawner = Arc::new(|_| {});
        let client = HostRpcClient::new(connection.clone(), spawner);

        client
            .send_fire_and_forget("chainSpec_v1_chainName", None)
            .unwrap();

        assert_eq!(connection.sent()[0]["params"], json!([]));
    }

    #[test]
    fn subxt_single_hash_unpin_is_normalized_for_host_providers() {
        let connection = TrackingConnection::new();
        let spawner: Spawner = Arc::new(|_| {});
        let client = HostRpcClient::new(connection.clone(), spawner);
        let params = RawValue::from_string(r#"["follow-id","0x1234"]"#.to_string()).unwrap();

        client
            .send_fire_and_forget("chainHead_v1_unpin", Some(params))
            .unwrap();

        assert_eq!(
            connection.sent()[0]["params"],
            json!(["follow-id", ["0x1234"]]),
        );
    }

    #[test]
    fn subscription_stream_holds_connection_lease_until_dropped() {
        let connection = TrackingConnection::new();
        let client = HostRpcClient::new(connection.clone(), thread_per_subscription_spawner());
        let rpc_client = RpcClient::new(client.clone());

        let subscription = block_on(rpc_client.subscribe::<Value>("sub", rpc_params![], "unsub"))
            .expect("subscription should start");

        drop(rpc_client);
        drop(client);
        assert_eq!(connection.close_count(), 0);

        drop(subscription);
        assert_eq!(connection.close_count(), 1);
    }

    fn raw(text: &str) -> Box<RawValue> {
        RawValue::from_string(text.to_string()).expect("valid json")
    }

    fn drain(rx: &mut mpsc::UnboundedReceiver<Result<Box<RawValue>, RpcError>>) -> Vec<String> {
        let mut seen = Vec::new();
        while let Ok(item) = rx.try_recv() {
            seen.push(match item {
                Ok(value) => value.get().to_string(),
                Err(error) => format!("error: {error}"),
            });
        }
        seen
    }

    /// Answers a subscribe by emitting a notification for the id *before* the
    /// ack, which is the order that produced `Stop` ahead of `Initialized`.
    struct RacingConnection {
        sender: Mutex<Option<mpsc::UnboundedSender<String>>>,
        receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    }

    impl RacingConnection {
        fn new() -> Arc<Self> {
            let (tx, rx) = mpsc::unbounded();
            Arc::new(Self {
                sender: Mutex::new(Some(tx)),
                receiver: Mutex::new(Some(rx)),
            })
        }

        fn emit(&self, frame: String) {
            if let Some(sender) = self.sender.lock().unwrap().as_ref() {
                let _ = sender.unbounded_send(frame);
            }
        }

        fn notify(&self, event: &str) {
            self.emit(
                json!({
                    "jsonrpc": "2.0",
                    "method": "sub",
                    "params": { "subscription": "sub-1", "result": { "event": event } },
                })
                .to_string(),
            );
        }
    }

    impl JsonRpcConnection for RacingConnection {
        fn send(&self, request: String) {
            let Ok(value) = serde_json::from_str::<Value>(&request) else {
                return;
            };
            let Some(id) = value.get("id").cloned() else {
                return;
            };
            if value.get("method").and_then(Value::as_str) == Some("sub") {
                // Ahead of the ack, so the router sees it with no sink yet.
                self.notify("buffered");
                self.emit(json!({ "jsonrpc": "2.0", "id": id, "result": "sub-1" }).to_string());
            }
        }

        fn responses(&self) -> BoxStream<'static, String> {
            self.receiver
                .lock()
                .unwrap()
                .take()
                .expect("responses called twice")
                .boxed()
        }

        fn close(&self) {
            self.sender.lock().unwrap().take();
        }
    }

    /// The regression this refactor exists for, driven through the real
    /// `subscribe` rather than a hand-rolled copy of its critical section: a
    /// notification that arrives before the ack is replayed, and stays ahead of
    /// one that arrives after.
    #[test]
    fn subscribe_replays_what_arrived_before_the_ack_and_keeps_it_first() {
        let connection = RacingConnection::new();
        let client = HostRpcClient::new(connection.clone(), thread_per_subscription_spawner());
        let rpc_client = RpcClient::new(client.clone());

        let mut subscription =
            block_on(rpc_client.subscribe::<Value>("sub", rpc_params![], "unsub"))
                .expect("subscription should start");

        connection.notify("live");

        // Read on a worker with a deadline: a subscription that never replays
        // would otherwise block this test forever, and a lost notification
        // should report as a failure rather than as a CI timeout.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let first = block_on(subscription.next());
            let second = block_on(subscription.next());
            let _ = done_tx.send((first, second));
        });
        let (first, second) = done_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("subscribe did not deliver both notifications");

        let first = first.expect("a buffered notification").expect("decodes");
        let second = second.expect("a live notification").expect("decodes");
        assert_eq!(first, json!({ "event": "buffered" }));
        assert_eq!(second, json!({ "event": "live" }));
    }

    /// Replay order, pinned without a thread: two buffered items and one that
    /// arrives after activation must come out in arrival order. The threaded
    /// test below only observes this when the deliverer wins the lock race, so
    /// the deterministic version is what actually guards it.
    #[test]
    fn buffered_events_replay_in_arrival_order() {
        let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
        let (tx, mut rx) = mpsc::unbounded();

        client
            .inner
            .deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"first"}"#));
        client
            .inner
            .deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"second"}"#));

        client
            .inner
            .subscriptions
            .lock()
            .unwrap()
            .activate("sub-1".to_string(), tx);

        client
            .inner
            .deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"live"}"#));

        assert_eq!(
            drain(&mut rx),
            vec![
                r#"{"event":"first"}"#.to_string(),
                r#"{"event":"second"}"#.to_string(),
                r#"{"event":"live"}"#.to_string(),
            ],
            "buffered events must replay oldest first, ahead of anything later"
        );
    }

    /// The lock is taken from `Drop`, so a poisoned one must not turn a failure
    /// somewhere else into a second panic during unwinding, which aborts the
    /// process rather than reporting anything.
    #[test]
    fn a_poisoned_subscription_lock_still_closes_cleanly() {
        let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
        let inner = Arc::clone(&client.inner);

        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = inner.subscriptions.lock().unwrap();
            panic!("poisoning the subscription lock on purpose");
        }));
        assert!(
            poisoned.is_err(),
            "the helper panic should have been caught"
        );
        assert!(
            client.inner.subscriptions.is_poisoned(),
            "the lock should be poisoned for this test to mean anything"
        );

        // Would panic a second time under `unwrap`, aborting the test binary.
        client
            .inner
            .close_with_error(client_error("peer went away"));
        client.inner.unsubscribe("sub-1", "unsub");
    }

    /// A notification delivered while activation is mid-flight waits for the
    /// same lock the replay runs under, so it lands behind it.
    #[test]
    fn a_live_notification_cannot_overtake_replay_in_progress() {
        let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
        let (tx, mut rx) = mpsc::unbounded();

        client.inner.deliver_or_buffer_subscription_item(
            "sub-1".to_string(),
            raw(r#"{"event":"buffered"}"#),
        );

        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let deliverer = {
            let inner = Arc::clone(&client.inner);
            std::thread::spawn(move || {
                let _ = ready_tx.send(());
                inner.deliver_or_buffer_subscription_item(
                    "sub-1".to_string(),
                    raw(r#"{"event":"live"}"#),
                );
            })
        };

        // Wait outside the critical section: a panic under the guard would
        // poison the lock and turn a failure into a process abort.
        ready_rx.recv().expect("deliverer started");
        client
            .inner
            .subscriptions
            .lock()
            .unwrap()
            .activate("sub-1".to_string(), tx.clone());

        deliverer.join().expect("deliverer finished");

        assert_eq!(
            drain(&mut rx),
            vec![
                r#"{"event":"buffered"}"#.to_string(),
                r#"{"event":"live"}"#.to_string(),
            ],
            "the live notification overtook the buffered one"
        );
    }

    /// Closing reports the failure to whoever is reading, but never ahead of
    /// events already queued for them.
    #[test]
    fn a_close_error_lands_behind_older_events() {
        let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
        let (tx, mut rx) = mpsc::unbounded();

        client
            .inner
            .deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"first"}"#));
        client
            .inner
            .subscriptions
            .lock()
            .unwrap()
            .activate("sub-1".to_string(), tx.clone());

        client
            .inner
            .close_with_error(client_error("peer went away"));

        let seen = drain(&mut rx);
        assert_eq!(seen.len(), 2, "expected the event and then the close error");
        assert_eq!(seen[0], r#"{"event":"first"}"#);
        assert!(
            seen[1].contains("json-rpc connection closed"),
            "close error should arrive last, got {:?}",
            seen[1]
        );
    }

    #[test]
    fn closing_drops_buffers_that_have_no_reader() {
        let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
        client
            .inner
            .deliver_or_buffer_subscription_item("orphan".to_string(), raw(r#"{"event":"x"}"#));

        client
            .inner
            .close_with_error(client_error("peer went away"));

        let remaining = client.inner.subscriptions.lock().unwrap().entries.len();
        assert_eq!(
            remaining, 0,
            "close should leave no subscription state behind"
        );
    }

    /// A frame off the wire reaches the subscription it names. Without this a
    /// dropped notification surfaces as a hung suite rather than a failure.
    #[test]
    fn a_notification_frame_is_routed_to_its_subscription() {
        let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
        let (tx, mut rx) = mpsc::unbounded();
        client
            .inner
            .subscriptions
            .lock()
            .unwrap()
            .activate("sub-1".to_string(), tx);

        client
            .inner
            .handle_frame(
                &json!({
                    "jsonrpc": "2.0",
                    "method": "sub",
                    "params": { "subscription": "sub-1", "result": { "event": "routed" } },
                })
                .to_string(),
            )
            .expect("frame parses");

        assert_eq!(drain(&mut rx), vec![r#"{"event":"routed"}"#.to_string()]);
    }

    /// Unsubscribe forgets the subscription and tells the peer.
    #[test]
    fn unsubscribe_forgets_the_subscription_and_tells_the_host() {
        let connection = TrackingConnection::new();
        let client = HostRpcClient::new(connection.clone(), Arc::new(|_| {}));
        let (tx, _rx) = mpsc::unbounded();
        client
            .inner
            .subscriptions
            .lock()
            .unwrap()
            .activate("sub-1".to_string(), tx);

        client.inner.unsubscribe("sub-1", "unsub");

        let remaining = client.inner.subscriptions.lock().unwrap().entries.len();
        assert_eq!(remaining, 0, "unsubscribe should forget the subscription");
        let sent = connection.sent();
        let last = sent.last().expect("an unsubscribe request was sent");
        assert_eq!(last.get("method").and_then(Value::as_str), Some("unsub"));
        assert_eq!(last.get("params"), Some(&json!(["sub-1"])));
    }

    #[test]
    fn dropping_the_subscription_stream_unsubscribes() {
        let connection = TrackingConnection::new();
        let client = HostRpcClient::new(connection.clone(), thread_per_subscription_spawner());
        let rpc_client = RpcClient::new(client.clone());

        let subscription = block_on(rpc_client.subscribe::<Value>("sub", rpc_params![], "unsub"))
            .expect("subscription should start");
        drop(subscription);

        let sent = connection.sent();
        assert!(
            sent.iter()
                .any(|frame| frame.get("method").and_then(Value::as_str) == Some("unsub")),
            "dropping the stream should unsubscribe, sent: {sent:?}"
        );
    }

    /// Re-using a live id replaces the sink rather than leaving the old one
    /// attached, so the newest subscriber is the one that receives.
    #[test]
    fn re_activating_an_id_replaces_the_sink() {
        let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
        let (first_tx, mut first_rx) = mpsc::unbounded();
        let (second_tx, mut second_rx) = mpsc::unbounded();
        {
            let mut state = client.inner.subscriptions.lock().unwrap();
            state.activate("sub-1".to_string(), first_tx);
            state.activate("sub-1".to_string(), second_tx);
        }

        client
            .inner
            .deliver_or_buffer_subscription_item("sub-1".to_string(), raw(r#"{"event":"x"}"#));

        assert_eq!(drain(&mut first_rx), Vec::<String>::new());
        assert_eq!(drain(&mut second_rx), vec![r#"{"event":"x"}"#.to_string()]);
    }

    /// A closed client refuses new work instead of hanging on a dead peer.
    #[test]
    fn a_closed_client_refuses_requests_and_subscriptions() {
        let client =
            HostRpcClient::new(TrackingConnection::new(), thread_per_subscription_spawner());
        let rpc_client = RpcClient::new(client.clone());
        client
            .inner
            .close_with_error(client_error("peer went away"));

        // Bounded: without the closed check these block on a peer that will
        // never answer, and a hung suite diagnoses nothing.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let request = block_on(rpc_client.request::<Value>("any", rpc_params![])).is_err();
            let subscribe =
                block_on(rpc_client.subscribe::<Value>("sub", rpc_params![], "unsub")).is_err();
            let _ = done_tx.send((request, subscribe));
        });
        let (request_failed, subscribe_failed) = done_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("a closed client should refuse rather than hang");

        assert!(request_failed, "a request on a closed client should fail");
        assert!(
            subscribe_failed,
            "a subscribe on a closed client should fail"
        );
    }

    /// The cap counts subscriptions holding a buffer. Asserted against the map
    /// itself rather than `pending_count`, so the counter cannot be its own
    /// oracle.
    #[test]
    fn the_buffer_cap_counts_only_subscriptions_holding_items() {
        let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
        let (tx, _rx) = mpsc::unbounded();
        client
            .inner
            .subscriptions
            .lock()
            .unwrap()
            .activate("active".to_string(), tx);

        for index in 0..MAX_BUFFERED_SUBSCRIPTIONS {
            client
                .inner
                .deliver_or_buffer_subscription_item(format!("sub-{index}"), raw("1"));
        }
        client
            .inner
            .deliver_or_buffer_subscription_item("one-too-many".to_string(), raw("1"));

        // Read out before asserting: a failed assertion under the guard poisons
        // the lock and the client's own `Drop` then aborts instead of reporting.
        let (total, active_survives, over_cap) = {
            let state = client.inner.subscriptions.lock().unwrap();
            (
                state.entries.len(),
                matches!(
                    state.entries.get("active"),
                    Some(SubscriptionEntry::Active(_))
                ),
                state.entries.contains_key("one-too-many"),
            )
        };
        assert_eq!(
            total,
            MAX_BUFFERED_SUBSCRIPTIONS + 1,
            "the active subscription must not consume a buffer slot"
        );
        assert!(
            active_survives,
            "the active subscription should still be active"
        );
        assert!(
            !over_cap,
            "a subscription past the cap must not be buffered"
        );
    }

    #[test]
    fn a_buffer_stops_growing_at_its_item_cap() {
        let client = HostRpcClient::new(TrackingConnection::new(), Arc::new(|_| {}));
        for _ in 0..MAX_BUFFERED_ITEMS_PER_SUBSCRIPTION + 8 {
            client
                .inner
                .deliver_or_buffer_subscription_item("sub-1".to_string(), raw("1"));
        }

        let buffered = {
            let state = client.inner.subscriptions.lock().unwrap();
            match state.entries.get("sub-1") {
                Some(SubscriptionEntry::Pending(items)) => items.len(),
                _ => usize::MAX,
            }
        };
        assert_eq!(buffered, MAX_BUFFERED_ITEMS_PER_SUBSCRIPTION);
    }
}
