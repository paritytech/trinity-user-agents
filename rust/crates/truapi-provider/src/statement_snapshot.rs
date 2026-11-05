//! Initial pages of a light-client statement subscription.
//!
//! A full node answers `statement_subscribeStatement` by replaying the matching
//! statements of its store in pages carrying `remaining`, ending with
//! `remaining: 0`, and only then sends live notifications. Callers such as a
//! lookup read the initial pages and stop at `remaining: 0`.
//!
//! smoldot keeps no store, so it sends an empty page with `remaining: 0` right
//! after the subscription id. Its peers do hold the statements: a subscription
//! changes the topic affinity smoldot advertises over `statement/2`, and a full
//! node replays every stored statement matching a newly received affinity.
//! Those arrive a moment later as live notifications without `remaining`, after
//! the caller has already concluded there is nothing stored.
//!
//! [`InitialPages`] restores the full-node contract on top of smoldot: it
//! withholds smoldot's empty page, collects the statements the peers replay for
//! the new subscription, and emits them as the initial pages once the replay
//! has settled, or once [`SnapshotTiming::window`] has passed. Live
//! notifications follow unchanged.

use core::future::Future as _;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use futures::stream::{BoxStream, Stream, StreamExt};
use futures_timer::Delay;
use serde_json::{Value, json};

const SUBSCRIBE_METHOD: &str = "statement_subscribeStatement";
const UNSUBSCRIBE_METHOD: &str = "statement_unsubscribeStatement";
const NOTIFICATION_METHOD: &str = "statement_statement";

/// Hex characters of statements one page may carry, matching the 4 MiB chunk
/// limit polkadot-sdk's statement RPC applies to its initial pages.
const PAGE_HEX_LIMIT: usize = 4 * 1024 * 1024;

/// How long the initial pages of a subscription wait for the peers' replay.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SnapshotTiming {
    /// Longest wait after the subscription id, whether or not anything came.
    pub window: Duration,
    /// Quiet period after the latest replayed statement that ends the wait
    /// before `window`.
    pub settle: Duration,
}

impl SnapshotTiming {
    /// smoldot advertises a changed affinity at once, or within its one-second
    /// update interval of the previous one. A full node applies received
    /// affinities on a one-second tick and then sends the matching statements
    /// in 10 ms bursts. Three seconds covers both ticks plus the round trip.
    pub(crate) const LIGHT_CLIENT: SnapshotTiming = SnapshotTiming {
        window: Duration::from_secs(3),
        settle: Duration::from_millis(500),
    };
}

/// Statement subscription requests in flight on one connection, recorded by
/// the request side and consumed by [`InitialPages`].
#[derive(Default)]
pub(crate) struct StatementRequests {
    inner: Mutex<PendingRequests>,
}

#[derive(Default)]
struct PendingRequests {
    /// JSON-encoded ids of `statement_subscribeStatement` requests whose
    /// response has not been seen yet.
    subscribes: HashSet<String>,
    /// Subscriptions the caller has closed, whose initial pages are dropped.
    unsubscribed: Vec<String>,
}

impl StatementRequests {
    /// Record `request` if it opens or closes a statement subscription.
    pub(crate) fn note(&self, request: &str) {
        // Every request crosses here; only statement ones are worth parsing.
        if !request.contains("statement_") {
            return;
        }
        let Ok(request) = serde_json::from_str::<Value>(request) else {
            return;
        };
        let mut pending = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match request.get("method").and_then(Value::as_str) {
            Some(SUBSCRIBE_METHOD) => {
                if let Some(id) = request.get("id") {
                    pending.subscribes.insert(id.to_string());
                }
            }
            Some(UNSUBSCRIBE_METHOD) => {
                let subscription = match request.get("params") {
                    Some(Value::Array(params)) => params.first(),
                    Some(Value::Object(params)) => params.get("subscription"),
                    _ => None,
                };
                if let Some(subscription) = subscription.and_then(Value::as_str) {
                    pending.unsubscribed.push(subscription.to_owned());
                }
            }
            _ => {}
        }
    }

    fn awaiting_subscription(&self) -> bool {
        !self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .subscribes
            .is_empty()
    }

    /// Whether `id` belongs to a subscribe request, forgetting it either way
    /// it is answered.
    fn take_subscribe(&self, id: &Value) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .subscribes
            .remove(&id.to_string())
    }

    fn take_unsubscribed(&self) -> Vec<String> {
        core::mem::take(
            &mut self
                .inner
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .unsubscribed,
        )
    }
}

/// Statements collected for one subscription before its initial pages go out.
struct Snapshot {
    /// Fires at the end of [`SnapshotTiming::window`].
    window: Delay,
    /// Fires [`SnapshotTiming::settle`] after the latest new statement.
    settle: Option<Delay>,
    statements: Vec<String>,
    seen: HashSet<String>,
}

impl Snapshot {
    fn is_due(&mut self, cx: &mut Context<'_>) -> bool {
        let settled = self
            .settle
            .as_mut()
            .is_some_and(|settle| Pin::new(settle).poll(cx).is_ready());
        // Both are polled so that both wake this task.
        Pin::new(&mut self.window).poll(cx).is_ready() || settled
    }

    /// The initial pages for `subscription`: page boundaries follow
    /// polkadot-sdk, and the last page, possibly empty, has `remaining: 0`.
    fn into_pages(self, subscription: &str) -> Vec<String> {
        let total = self.statements.len();
        let mut pages = Vec::new();
        let mut page = Vec::new();
        let mut page_hex = 0;
        let mut sent = 0;
        for statement in self.statements {
            if !page.is_empty() && page_hex + statement.len() > PAGE_HEX_LIMIT {
                sent += page.len();
                pages.push(page_frame(
                    subscription,
                    core::mem::take(&mut page),
                    total - sent,
                ));
                page_hex = 0;
            }
            page_hex += statement.len();
            page.push(statement);
        }
        pages.push(page_frame(subscription, page, 0));
        pages
    }
}

fn page_frame(subscription: &str, statements: Vec<String>, remaining: usize) -> String {
    json!({
        "jsonrpc": "2.0",
        "method": NOTIFICATION_METHOD,
        "params": {
            "subscription": subscription,
            "result": {
                "event": "newStatements",
                "data": { "statements": statements, "remaining": remaining },
            },
        },
    })
    .to_string()
}

/// Response stream of a light-client connection with the full-node initial
/// pages restored. See the module documentation.
pub(crate) struct InitialPages {
    frames: BoxStream<'static, String>,
    requests: Arc<StatementRequests>,
    timing: SnapshotTiming,
    /// Subscriptions whose initial pages are still being collected.
    open: HashMap<String, Snapshot>,
    /// Frames ready to hand out, ahead of anything new.
    ready: VecDeque<String>,
}

impl InitialPages {
    pub(crate) fn new(
        frames: BoxStream<'static, String>,
        requests: Arc<StatementRequests>,
        timing: SnapshotTiming,
    ) -> Self {
        InitialPages {
            frames,
            requests,
            timing,
            open: HashMap::new(),
            ready: VecDeque::new(),
        }
    }

    /// Route one frame from smoldot, returning it unless it is absorbed into
    /// a snapshot.
    fn route(&mut self, frame: String) -> Option<String> {
        let response = self.requests.awaiting_subscription();
        let notification = !self.open.is_empty() && frame.contains(NOTIFICATION_METHOD);
        if !response && !notification {
            return Some(frame);
        }
        let Ok(value) = serde_json::from_str::<Value>(&frame) else {
            return Some(frame);
        };
        if let Some(id) = value.get("id") {
            if self.requests.take_subscribe(id)
                && let Some(subscription) = value.get("result").and_then(Value::as_str)
            {
                self.open.insert(
                    subscription.to_owned(),
                    Snapshot {
                        window: Delay::new(self.timing.window),
                        settle: None,
                        statements: Vec::new(),
                        seen: HashSet::new(),
                    },
                );
            }
            return Some(frame);
        }
        if value.get("method").and_then(Value::as_str) != Some(NOTIFICATION_METHOD) {
            return Some(frame);
        }
        let params = &value["params"];
        let Some(snapshot) = params["subscription"]
            .as_str()
            .and_then(|subscription| self.open.get_mut(subscription))
        else {
            return Some(frame);
        };
        // smoldot's own empty page and the peers' replay alike.
        let mut added = false;
        for statement in params["result"]["data"]["statements"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if snapshot.seen.insert(statement.to_owned()) {
                snapshot.statements.push(statement.to_owned());
                added = true;
            }
        }
        if added {
            match &mut snapshot.settle {
                Some(settle) => settle.reset(self.timing.settle),
                None => snapshot.settle = Some(Delay::new(self.timing.settle)),
            }
        }
        None
    }
}

impl Stream for InitialPages {
    type Item = String;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<String>> {
        let this = self.get_mut();
        loop {
            if let Some(frame) = this.ready.pop_front() {
                return Poll::Ready(Some(frame));
            }
            for subscription in this.requests.take_unsubscribed() {
                this.open.remove(&subscription);
            }
            let due: Vec<String> = this
                .open
                .iter_mut()
                .filter_map(|(subscription, snapshot)| {
                    snapshot.is_due(cx).then(|| subscription.clone())
                })
                .collect();
            for subscription in due {
                if let Some(snapshot) = this.open.remove(&subscription) {
                    this.ready.extend(snapshot.into_pages(&subscription));
                }
            }
            if !this.ready.is_empty() {
                continue;
            }
            match this.frames.poll_next_unpin(cx) {
                Poll::Ready(Some(frame)) => {
                    if let Some(frame) = this.route(frame) {
                        return Poll::Ready(Some(frame));
                    }
                }
                // The connection is closed; nothing would read the pages.
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use futures::channel::mpsc;
    use futures::executor::block_on;

    const TIMING: SnapshotTiming = SnapshotTiming {
        window: Duration::from_millis(400),
        settle: Duration::from_millis(100),
    };

    /// What smoldot sends for a subscription: the id, then its empty page.
    fn subscribed(id: u64, subscription: &str) -> [String; 2] {
        [
            json!({"jsonrpc":"2.0","id":id,"result":subscription}).to_string(),
            page_frame(subscription, Vec::new(), 0),
        ]
    }

    /// A live notification as smoldot forwards a peer's statements.
    fn gossip(subscription: &str, statements: &[&str]) -> String {
        json!({
            "jsonrpc": "2.0",
            "method": NOTIFICATION_METHOD,
            "params": {
                "subscription": subscription,
                "result": { "event": "newStatements", "data": { "statements": statements } },
            },
        })
        .to_string()
    }

    fn subscribe_request(id: u64) -> String {
        json!({"jsonrpc":"2.0","id":id,"method":SUBSCRIBE_METHOD,"params":[{"matchAll":["0x01"]}]})
            .to_string()
    }

    fn harness() -> (
        mpsc::UnboundedSender<String>,
        Arc<StatementRequests>,
        InitialPages,
    ) {
        let (tx, rx) = mpsc::unbounded();
        let requests = Arc::new(StatementRequests::default());
        let pages = InitialPages::new(rx.boxed(), Arc::clone(&requests), TIMING);
        (tx, requests, pages)
    }

    fn data(frame: &str) -> Value {
        let frame: Value = serde_json::from_str(frame).expect("valid JSON");
        frame["params"]["result"]["data"].clone()
    }

    /// The regression: statements the peers replay after smoldot's empty page
    /// make up the initial page, which ends with `remaining: 0`, and later
    /// statements stay live notifications.
    #[test]
    fn replayed_statements_form_the_initial_page() {
        let (tx, requests, mut pages) = harness();
        requests.note(&subscribe_request(7));
        for frame in subscribed(7, "sub") {
            tx.unbounded_send(frame).unwrap();
        }
        tx.unbounded_send(gossip("sub", &["0xaa", "0xbb"])).unwrap();
        tx.unbounded_send(gossip("sub", &["0xbb", "0xcc"])).unwrap();

        let response: Value = serde_json::from_str(&block_on(pages.next()).unwrap()).unwrap();
        assert_eq!(response["result"], "sub");
        let initial = data(&block_on(pages.next()).unwrap());
        assert_eq!(initial["statements"], json!(["0xaa", "0xbb", "0xcc"]));
        assert_eq!(initial["remaining"], 0);

        tx.unbounded_send(gossip("sub", &["0xdd"])).unwrap();
        let live = data(&block_on(pages.next()).unwrap());
        assert_eq!(live["statements"], json!(["0xdd"]));
        assert!(
            live.get("remaining").is_none(),
            "live notifications keep no remaining"
        );
    }

    /// Nothing replayed: the empty page still comes, once the window ends.
    #[test]
    fn an_empty_snapshot_ends_with_the_window() {
        let (tx, requests, mut pages) = harness();
        requests.note(&subscribe_request(1));
        for frame in subscribed(1, "sub") {
            tx.unbounded_send(frame).unwrap();
        }
        block_on(pages.next()).unwrap();
        let started = std::time::Instant::now();
        let initial = data(&block_on(pages.next()).unwrap());
        assert!(started.elapsed() >= TIMING.window - Duration::from_millis(50));
        assert_eq!(initial["statements"], json!([]));
        assert_eq!(initial["remaining"], 0);
    }

    /// A replay ends the wait once it has been quiet for the settle period.
    #[test]
    fn a_settled_replay_ends_before_the_window() {
        let (tx, requests, mut pages) = harness();
        requests.note(&subscribe_request(1));
        for frame in subscribed(1, "sub") {
            tx.unbounded_send(frame).unwrap();
        }
        tx.unbounded_send(gossip("sub", &["0xaa"])).unwrap();
        block_on(pages.next()).unwrap();
        let started = std::time::Instant::now();
        let initial = data(&block_on(pages.next()).unwrap());
        assert!(started.elapsed() < TIMING.window);
        assert_eq!(initial["statements"], json!(["0xaa"]));
    }

    /// Other traffic, including other subscriptions' notifications, is not
    /// held behind a snapshot.
    #[test]
    fn unrelated_frames_pass_through() {
        let (tx, requests, mut pages) = harness();
        requests.note(&subscribe_request(1));
        for frame in subscribed(1, "sub") {
            tx.unbounded_send(frame).unwrap();
        }
        let other = gossip("other", &["0xee"]);
        tx.unbounded_send(other.clone()).unwrap();
        let reply = json!({"jsonrpc":"2.0","id":2,"result":"Paseo"}).to_string();
        tx.unbounded_send(reply.clone()).unwrap();
        block_on(pages.next()).unwrap();
        assert_eq!(block_on(pages.next()).unwrap(), other);
        assert_eq!(block_on(pages.next()).unwrap(), reply);
    }

    /// A subscription closed before its pages go out gets none.
    #[test]
    fn an_unsubscribed_snapshot_is_dropped() {
        let (tx, requests, mut pages) = harness();
        requests.note(&subscribe_request(1));
        for frame in subscribed(1, "sub") {
            tx.unbounded_send(frame).unwrap();
        }
        tx.unbounded_send(gossip("sub", &["0xaa"])).unwrap();
        block_on(pages.next()).unwrap();
        // Takes in the empty page and the replay, and has nothing to hand out.
        assert!(pages.next().now_or_never().is_none());
        assert!(pages.open["sub"].seen.contains("0xaa"));
        requests.note(
            &json!({"jsonrpc":"2.0","id":2,"method":UNSUBSCRIBE_METHOD,"params":["sub"]})
                .to_string(),
        );
        let answer = json!({"jsonrpc":"2.0","id":2,"result":true}).to_string();
        tx.unbounded_send(answer.clone()).unwrap();
        assert_eq!(block_on(pages.next()).unwrap(), answer);
        drop(tx);
        assert_eq!(block_on(pages.next()), None);
    }

    /// A refused subscribe opens no snapshot.
    #[test]
    fn a_failed_subscribe_holds_nothing() {
        let (tx, requests, mut pages) = harness();
        requests.note(&subscribe_request(1));
        tx.unbounded_send(
            json!({"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"bad"}}).to_string(),
        )
        .unwrap();
        block_on(pages.next()).unwrap();
        assert!(!requests.awaiting_subscription());
        assert!(pages.open.is_empty());
    }

    /// Pages split at polkadot-sdk's chunk size and count down `remaining`.
    #[test]
    fn large_snapshots_are_paged_like_a_full_node() {
        let statement = format!("0x{}", "ab".repeat(PAGE_HEX_LIMIT / 2 / 3));
        let snapshot = Snapshot {
            window: Delay::new(TIMING.window),
            settle: None,
            statements: vec![statement; 4],
            seen: HashSet::new(),
        };
        let pages: Vec<Value> = snapshot.into_pages("sub").iter().map(|p| data(p)).collect();
        let shape: Vec<(usize, u64)> = pages
            .iter()
            .map(|page| {
                (
                    page["statements"].as_array().unwrap().len(),
                    page["remaining"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(shape, [(2, 2), (2, 0)]);
    }
}
