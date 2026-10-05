use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::executor::block_on;
use parity_scale_codec::Encode;
use serde_json::{Value, json};
use sp_crypto_hashing::blake2_256;
use subxt::config::substrate::{Digest, SubstrateHeader};
use subxt::tx::{TransactionInvalid, ValidationResult};
use subxt::utils::H256;

use super::{
    BlockBackend, ChainHeads, DispatchOutcome, EncodedExtrinsic, HashAndNumber, HeadEvent, Heads,
    TxSubmitter, TxValidator, WatchEvent,
};
use crate::chain_runtime::{ChainRuntime, RuntimeFailureKind};
use crate::host_internal::extrinsic::tests::{
    bulletin_chain_state, bulletin_runtime_call, system_events,
};
use crate::test_support::{ScriptedProvider, notification_sender, test_spawner, wait_for_sent};

const GENESIS: H256 = H256([0xab; 32]);
const FOLLOW_ID: &str = "follow-1";
const FINALIZED_SUBSCRIPTION: &str = "finalized-heads";
const BEST_SUBSCRIPTION: &str = "best-heads";

/// A linear chain the scripted node serves over both the legacy methods and
/// chainHead, plus the scripted outcome of validation and submission.
struct Node {
    headers: Vec<SubstrateHeader<H256>>,
    finalized: u64,
    best: u64,
    bodies: HashMap<H256, Vec<Vec<u8>>>,
    events: HashMap<H256, Vec<u8>>,
    /// Heights `chain_getBlockHash` answers `null` for although they exist,
    /// as a light client does for every height it cannot verify.
    unservable_heights: HashSet<u64>,
    /// Blocks whose header the node serves but whose body it answers `null`
    /// for, as a pruned or light node does.
    unservable_bodies: HashSet<H256>,
    validation: Validation,
    /// Follow events announcing the blocks the submitted extrinsic lands in;
    /// the chainHead backend reports an inclusion only once it saw the block.
    follow_events: Vec<Value>,
    watch_events: Vec<Value>,
    next_operation: usize,
}

impl Node {
    /// Blocks `0..=best`, with `finalized` finalized, no bodies and no events.
    fn new(finalized: u64, best: u64) -> Self {
        let mut headers: Vec<SubstrateHeader<H256>> = Vec::new();
        for number in 0..=best {
            let parent_hash = headers.last().map(header_hash).unwrap_or_default();
            headers.push(header(number, parent_hash));
        }
        Self {
            headers,
            finalized,
            best,
            bodies: HashMap::new(),
            events: HashMap::new(),
            unservable_heights: HashSet::new(),
            unservable_bodies: HashSet::new(),
            validation: Validation::Valid,
            follow_events: Vec::new(),
            watch_events: Vec::new(),
            next_operation: 0,
        }
    }

    fn block(&self, number: u64) -> HashAndNumber {
        HashAndNumber {
            hash: header_hash(&self.headers[number as usize]),
            number,
        }
    }

    fn header_at(&self, hash: &Value) -> Option<&SubstrateHeader<H256>> {
        let hash = parse_hash(hash)?;
        self.headers
            .iter()
            .find(|header| header_hash(header) == hash)
    }

    fn runtime_call(&self, method: &str, at: &Value) -> Vec<u8> {
        if method != "TaggedTransactionQueue_validate_transaction" {
            return bulletin_runtime_call(method).unwrap();
        }
        let valid = match self.validation {
            Validation::Valid => true,
            Validation::Invalid => false,
            Validation::ValidOnlyAtBest => parse_hash(at) == Some(self.block(self.best).hash),
        };
        validation_output(valid)
    }

    fn respond(&mut self, request: &str) -> Vec<String> {
        let request: Value = serde_json::from_str(request).unwrap();
        let id = request["id"].clone();
        let params = &request["params"];
        let response =
            |result: Value| json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
        match request["method"].as_str().unwrap() {
            "chain_getBlockHash" => {
                let number = match &params[0] {
                    Value::Null => Some(self.best),
                    Value::Number(number) => number.as_u64(),
                    Value::String(hex) => {
                        u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok()
                    }
                    other => panic!("unexpected block number {other}"),
                };
                let hash = number
                    .filter(|number| {
                        *number <= self.best && !self.unservable_heights.contains(number)
                    })
                    .map(|number| json!(self.block(number).hash));
                vec![response(hash.unwrap_or(Value::Null))]
            }
            "chain_getHeader" => {
                let header = match &params[0] {
                    Value::Null => self.headers.last(),
                    hash => self.header_at(hash),
                };
                vec![response(json!(header))]
            }
            "chain_getBlock" => {
                let block = self
                    .header_at(&params[0])
                    .filter(|header| !self.unservable_bodies.contains(&header_hash(header)))
                    .map(|header| {
                    let extrinsics: Vec<String> = self
                        .bodies
                        .get(&header_hash(header))
                        .into_iter()
                        .flatten()
                        .map(|extrinsic| format!("0x{}", hex::encode(extrinsic)))
                        .collect();
                    json!({"block": {"header": header, "extrinsics": extrinsics}, "justifications": null})
                });
                vec![response(block.unwrap_or(Value::Null))]
            }
            "chain_getFinalizedHead" => vec![response(json!(self.block(self.finalized).hash))],
            "state_call" => {
                let output = self.runtime_call(params[0].as_str().unwrap(), &params[2]);
                vec![response(json!(format!("0x{}", hex::encode(output))))]
            }
            "state_getStorage" => {
                let events = parse_hash(&params[1])
                    .and_then(|at| self.events.get(&at))
                    .map(|events| json!(format!("0x{}", hex::encode(events))));
                vec![response(events.unwrap_or(Value::Null))]
            }
            "chain_subscribeFinalizedHeads" => vec![response(json!(FINALIZED_SUBSCRIPTION))],
            "chain_subscribeNewHeads" => vec![response(json!(BEST_SUBSCRIPTION))],
            "chainHead_v1_follow" => vec![
                response(json!(FOLLOW_ID)),
                follow_event(json!({
                    "event": "initialized",
                    "finalizedBlockHashes": [self.block(self.finalized).hash],
                    "finalizedBlockRuntime": null
                })),
            ],
            "chainHead_v1_header" => {
                let header = self
                    .header_at(&params[1])
                    .map(|header| json!(format!("0x{}", hex::encode(header.encode()))));
                vec![response(header.unwrap_or(Value::Null))]
            }
            "chainHead_v1_call" => {
                self.next_operation += 1;
                let operation_id = format!("call-{}", self.next_operation);
                let output = self.runtime_call(params[2].as_str().unwrap(), &params[1]);
                vec![
                    response(json!({"result": "started", "operationId": operation_id})),
                    follow_event(json!({
                        "event": "operationCallDone",
                        "operationId": operation_id,
                        "output": format!("0x{}", hex::encode(output))
                    })),
                ]
            }
            "transactionWatch_v1_submitAndWatch" => {
                let mut frames = vec![response(json!("tx-1"))];
                frames.extend(self.follow_events.iter().cloned().map(follow_event));
                frames.extend(self.watch_events.iter().map(|event| {
                    json!({
                        "jsonrpc": "2.0",
                        "method": "transactionWatch_v1_watchEvent",
                        "params": {"subscription": "tx-1", "result": event}
                    })
                    .to_string()
                }));
                frames
            }
            "chainHead_v1_unpin"
            | "chainHead_v1_stopOperation"
            | "transactionWatch_v1_unwatch"
            | "chain_unsubscribeFinalizedHeads"
            | "chain_unsubscribeNewHeads" => vec![response(Value::Null)],
            other => vec![
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": format!("unexpected method {other}")}
                })
                .to_string(),
            ],
        }
    }
}

/// What `TaggedTransactionQueue_validate_transaction` answers.
#[derive(Clone, Copy)]
enum Validation {
    Valid,
    Invalid,
    /// Valid against the best block's state only, like a transaction whose
    /// mortality is anchored above the finalized block.
    ValidOnlyAtBest,
}

fn header(number: u64, parent_hash: H256) -> SubstrateHeader<H256> {
    SubstrateHeader {
        parent_hash,
        number,
        state_root: H256::zero(),
        extrinsics_root: H256::zero(),
        digest: Digest::default(),
    }
}

/// Blake2-256 of the SCALE header: the hash the chain gives the block.
fn header_hash(header: &SubstrateHeader<H256>) -> H256 {
    H256(blake2_256(&header.encode()))
}

fn parse_hash(value: &Value) -> Option<H256> {
    let bytes = hex::decode(value.as_str()?.trim_start_matches("0x")).ok()?;
    (bytes.len() == 32).then(|| H256::from_slice(&bytes))
}

fn follow_event(result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "method": "chainHead_v1_followEvent",
        "params": {"subscription": FOLLOW_ID, "result": result}
    })
    .to_string()
}

fn head_notification(subscription: &str, header: &SubstrateHeader<H256>) -> String {
    json!({
        "jsonrpc": "2.0",
        "method": "chain_newHead",
        "params": {"subscription": subscription, "result": header}
    })
    .to_string()
}

/// `TaggedTransactionQueue_validate_transaction` output: a valid transaction,
/// or `Invalid(Payment)`.
fn validation_output(valid: bool) -> Vec<u8> {
    if !valid {
        return vec![1, 0, 1];
    }
    let mut output = vec![0];
    0u64.encode_to(&mut output);
    Vec::<Vec<u8>>::new().encode_to(&mut output);
    Vec::<Vec<u8>>::new().encode_to(&mut output);
    64u64.encode_to(&mut output);
    true.encode_to(&mut output);
    output
}

/// A scripted node for `node` and the capabilities over it.
fn serve(node: Node) -> (Arc<Mutex<Node>>, Arc<ScriptedProvider>, ChainRuntime) {
    let node = Arc::new(Mutex::new(node));
    let scripted = node.clone();
    let provider = Arc::new(ScriptedProvider::with_frames(move |request| {
        scripted.lock().unwrap().respond(request)
    }));
    let chain = ChainRuntime::new(provider.clone(), test_spawner());
    (node, provider, chain)
}

fn method_count(provider: &ScriptedProvider, method: &str) -> usize {
    provider
        .sent
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request.contains(method))
        .count()
}

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Poll `events` on a thread, since the node only announces heads after the
/// subscriptions are open, and hand back the first `count` of them.
fn collect_in_background(
    mut events: futures::stream::BoxStream<
        'static,
        Result<HeadEvent, crate::chain_runtime::RuntimeFailure>,
    >,
    count: usize,
) -> std::sync::mpsc::Receiver<Vec<HeadEvent>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let received = block_on(async {
            let mut received = Vec::new();
            while received.len() < count {
                received.push(events.next().await.unwrap().unwrap());
            }
            received
        });
        let _ = sender.send(received);
    });
    receiver
}

fn wait_for_subscriptions(provider: &ScriptedProvider) {
    let subscribed = |sent: &[String]| {
        sent.iter()
            .any(|request| request.contains("chain_subscribeFinalizedHeads"))
            && sent
                .iter()
                .any(|request| request.contains("chain_subscribeNewHeads"))
    };
    assert!(
        subscribed(&wait_for_sent(provider, subscribed)),
        "head subscriptions not opened"
    );
}

fn extrinsic(tag: u8) -> EncodedExtrinsic {
    EncodedExtrinsic::new(vec![0x10, tag, tag, tag, tag])
}

#[test]
fn extrinsic_hash_is_the_hash_the_chain_reports() {
    let bytes = extrinsic(1).bytes().to_vec();
    let offline = bulletin_chain_state().client_at(0).unwrap();
    let chain_hash = offline.tx().from_bytes(bytes.clone()).hash();

    assert_eq!(EncodedExtrinsic::new(bytes).hash(), chain_hash);
}

#[test]
fn heads_reports_finalized_and_best_with_their_numbers() {
    let (node, _, chain) = serve(Node::new(3, 5));
    let expected = {
        let node = node.lock().unwrap();
        Heads {
            finalized: node.block(3),
            best: node.block(5),
        }
    };

    assert_eq!(block_on(chain.heads(GENESIS)).unwrap(), expected);
}

#[test]
fn block_hash_resolves_historical_heights_and_is_none_past_the_head() {
    // Blocks far behind the finalized head are exactly what chainHead cannot
    // serve and the durable engine's canonicality check needs.
    let (node, _, chain) = serve(Node::new(8, 9));
    let first = node.lock().unwrap().block(1).hash;

    assert_eq!(block_on(chain.block_hash(GENESIS, 1)).unwrap(), Some(first));
    assert_eq!(block_on(chain.block_hash(GENESIS, 42)).unwrap(), None);
}

#[test]
fn block_number_is_read_from_the_header_and_none_for_an_unknown_block() {
    let (node, _, chain) = serve(Node::new(3, 5));
    let fourth = node.lock().unwrap().block(4).hash;

    assert_eq!(
        block_on(chain.block_number(GENESIS, fourth)).unwrap(),
        Some(4)
    );
    assert_eq!(
        block_on(chain.block_number(GENESIS, H256([7; 32]))).unwrap(),
        None
    );
}

#[test]
fn extrinsic_hashes_follow_block_order_and_are_none_for_an_unknown_block() {
    let mut node = Node::new(3, 5);
    let at = node.block(2).hash;
    node.bodies.insert(
        at,
        vec![extrinsic(1).bytes().to_vec(), extrinsic(2).bytes().to_vec()],
    );
    let (_, _, chain) = serve(node);

    assert_eq!(
        block_on(chain.extrinsic_hashes(GENESIS, at)).unwrap(),
        Some(vec![extrinsic(1).hash(), extrinsic(2).hash()]),
    );
    assert_eq!(
        block_on(chain.extrinsic_hashes(GENESIS, H256([7; 32]))).unwrap(),
        None
    );
}

#[test]
fn dispatch_outcome_reads_the_events_of_that_extrinsics_index() {
    // Events of every extrinsic share one storage value; reading another
    // extrinsic's event would report the wrong outcome.
    let mut node = Node::new(3, 5);
    let at = node.block(2);
    node.bodies.insert(
        at.hash,
        vec![extrinsic(1).bytes().to_vec(), extrinsic(2).bytes().to_vec()],
    );
    node.events.insert(
        at.hash,
        system_events(&[(0, "ExtrinsicSuccess"), (1, "ExtrinsicFailed")]),
    );
    let (_, _, chain) = serve(node);

    let outcomes = [extrinsic(1), extrinsic(2), extrinsic(3)]
        .map(|extrinsic| block_on(chain.dispatch_outcome(GENESIS, at, extrinsic.hash())).unwrap());

    assert_eq!(
        outcomes,
        [
            Some(DispatchOutcome::Succeeded),
            Some(DispatchOutcome::Failed),
            None
        ],
    );
}

#[test]
fn dispatch_outcome_without_a_dispatch_event_is_an_error() {
    // An included extrinsic always emits ExtrinsicSuccess or ExtrinsicFailed.
    // Missing both means the events can't be trusted, not that it failed.
    let mut node = Node::new(3, 5);
    let at = node.block(2);
    node.bodies
        .insert(at.hash, vec![extrinsic(1).bytes().to_vec()]);
    let (_, _, chain) = serve(node);

    assert!(block_on(chain.dispatch_outcome(GENESIS, at, extrinsic(1).hash())).is_err());
}

#[test]
fn head_events_report_finalized_and_best_blocks() {
    let (node, provider, chain) = serve(Node::new(3, 5));
    let events = block_on(chain.head_events(GENESIS)).unwrap();
    // The streams subscribe when first polled, so poll them before the node
    // announces anything.
    let received = collect_in_background(events, 2);
    wait_for_subscriptions(&provider);
    let (finalized, best) = {
        let node = node.lock().unwrap();
        (node.headers[4].clone(), node.headers[5].clone())
    };
    let notifications = notification_sender(&provider);
    notifications
        .unbounded_send(head_notification(FINALIZED_SUBSCRIPTION, &finalized))
        .unwrap();
    notifications
        .unbounded_send(head_notification(BEST_SUBSCRIPTION, &best))
        .unwrap();

    let mut received = received.recv_timeout(TIMEOUT).expect("two head events");
    received.sort_by_key(|event| matches!(event, HeadEvent::Best(_)));
    let node = node.lock().unwrap();

    assert_eq!(
        received,
        vec![
            HeadEvent::Finalized(node.block(4)),
            HeadEvent::Best(node.block(5))
        ],
    );
}

#[test]
fn validate_reports_the_pool_verdict() {
    let (node, _, chain) = serve(Node::new(3, 5));
    let valid = block_on(chain.validate(GENESIS, &extrinsic(1))).unwrap();
    node.lock().unwrap().validation = Validation::Invalid;
    let invalid = block_on(chain.validate(GENESIS, &extrinsic(1))).unwrap();

    assert!(valid.is_valid());
    assert_eq!(
        invalid,
        ValidationResult::Invalid(TransactionInvalid::Payment)
    );
}

#[test]
fn submit_and_watch_sends_the_bytes_once_and_ends_after_finalization() {
    let mut node = Node::new(3, 5);
    let (third, first, second) = (node.block(3).hash, node.block(4).hash, node.block(5).hash);
    node.follow_events = vec![
        json!({"event": "newBlock", "blockHash": first, "parentBlockHash": third, "newRuntime": null}),
        json!({"event": "newBlock", "blockHash": second, "parentBlockHash": first, "newRuntime": null}),
        json!({"event": "bestBlockChanged", "bestBlockHash": second}),
        json!({"event": "finalized", "finalizedBlockHashes": [first, second], "prunedBlockHashes": []}),
    ];
    node.watch_events = vec![
        json!({"event": "bestChainBlockIncluded", "block": {"hash": first, "index": "0"}}),
        json!({"event": "bestChainBlockIncluded", "block": null}),
        json!({"event": "bestChainBlockIncluded", "block": {"hash": second, "index": "0"}}),
        json!({"event": "finalized", "block": {"hash": second, "index": "0"}}),
        json!({"event": "dropped", "error": "after finalization"}),
    ];
    let (_, provider, chain) = serve(node);

    let events: Vec<WatchEvent> = block_on(
        block_on(chain.submit_and_watch(GENESIS, &extrinsic(1)))
            .unwrap()
            .collect(),
    );

    assert_eq!(
        events,
        vec![
            WatchEvent::InBestBlock(first),
            WatchEvent::NoLongerInBestBlock,
            WatchEvent::InBestBlock(second),
            WatchEvent::InFinalizedBlock(second),
        ],
    );
    let submitted = format!("0x{}", hex::encode(extrinsic(1).bytes()));
    let sent = provider.sent.lock().unwrap().clone();
    assert_eq!(
        sent.iter()
            .filter(|request| request.contains("transactionWatch_v1_submitAndWatch"))
            .map(|request| request.contains(&submitted))
            .collect::<Vec<_>>(),
        vec![true],
    );
}

#[test]
fn submit_and_watch_ends_when_the_pool_rejects_the_extrinsic() {
    let mut node = Node::new(3, 5);
    node.watch_events = vec![json!({"event": "invalid", "error": "scripted invalid"})];
    let (_, _, chain) = serve(node);

    let events: Vec<WatchEvent> = block_on(
        block_on(chain.submit_and_watch(GENESIS, &extrinsic(1)))
            .unwrap()
            .collect(),
    );

    assert_eq!(events, vec![WatchEvent::Invalid("scripted invalid".into())]);
}

#[test]
fn reads_and_submission_share_one_metadata_download() {
    // Both subxt clients are built from one chain config, so the metadata the
    // legacy reads load is the metadata submission uses.
    let mut node = Node::new(3, 5);
    let at = node.block(2);
    node.bodies
        .insert(at.hash, vec![extrinsic(1).bytes().to_vec()]);
    node.events
        .insert(at.hash, system_events(&[(0, "ExtrinsicSuccess")]));
    node.watch_events = vec![json!({"event": "invalid", "error": "scripted invalid"})];
    let (_, provider, chain) = serve(node);

    block_on(chain.dispatch_outcome(GENESIS, at, extrinsic(1).hash())).unwrap();
    let _: Vec<WatchEvent> = block_on(
        block_on(chain.submit_and_watch(GENESIS, &extrinsic(1)))
            .unwrap()
            .collect(),
    );

    assert_eq!(method_count(&provider, "Metadata_metadata_at_version"), 1);
}

#[test]
fn block_hash_is_an_error_when_the_node_cannot_serve_a_finalized_height() {
    // A light client answers null for every height it cannot verify. Taking
    // that as "no such block" would make every recorded inclusion look
    // reorged out.
    let mut node = Node::new(8, 9);
    node.unservable_heights.insert(2);
    let (_, _, chain) = serve(node);

    assert!(block_on(chain.block_hash(GENESIS, 2)).is_err());
}

#[test]
fn a_known_block_whose_body_the_node_cannot_serve_is_an_error() {
    // A pruned or light node knows the header but answers null for the body;
    // that does not prove the extrinsic is absent.
    let mut node = Node::new(3, 5);
    let at = node.block(2);
    node.unservable_bodies.insert(at.hash);
    let (_, _, chain) = serve(node);

    assert!(block_on(chain.extrinsic_hashes(GENESIS, at.hash)).is_err());
    assert!(block_on(chain.dispatch_outcome(GENESIS, at, extrinsic(1).hash())).is_err());
}

#[test]
fn head_events_report_only_blocks_the_node_announced() {
    // Filling a finality gap by height asks for hashes a light client cannot
    // give; a null answer must never turn into the best block reported as
    // finalized.
    let mut node = Node::new(3, 7);
    node.unservable_heights.extend([4, 5]);
    let (node, provider, chain) = serve(node);
    let events = block_on(chain.head_events(GENESIS)).unwrap();
    let received = collect_in_background(events, 1);
    wait_for_subscriptions(&provider);
    let finalized = node.lock().unwrap().headers[6].clone();
    notification_sender(&provider)
        .unbounded_send(head_notification(FINALIZED_SUBSCRIPTION, &finalized))
        .unwrap();

    let received = received.recv_timeout(TIMEOUT).expect("a head event");

    assert_eq!(
        received,
        vec![HeadEvent::Finalized(node.lock().unwrap().block(6))]
    );
}

#[test]
fn validate_checks_against_the_best_block() {
    // The pool validates at best; checking at finalized rejects extrinsics
    // whose mortality is anchored at an unfinalized best block.
    let mut node = Node::new(3, 5);
    node.validation = Validation::ValidOnlyAtBest;
    let (_, _, chain) = serve(node);

    assert!(
        block_on(chain.validate(GENESIS, &extrinsic(1)))
            .unwrap()
            .is_valid()
    );
}

#[test]
fn a_closed_transport_is_unavailable_rather_than_a_node_failure() {
    // Callers retry an unavailable chain later but give up on a node that
    // answered; a dead connection is the former.
    let (_, provider, chain) = serve(Node::new(3, 5));
    provider.sender.lock().unwrap().take();

    let failure = block_on(chain.block_hash(GENESIS, 1)).unwrap_err();

    assert_eq!(failure.kind(), RuntimeFailureKind::Unavailable);
}

#[test]
fn submit_and_watch_ends_when_the_node_stops_tracking() {
    // A node that gives up on the watch reports it as an error; the extrinsic
    // may still land, so the stream ends and leaves the outcome to recovery.
    let mut node = Node::new(3, 5);
    node.watch_events = vec![json!({"event": "error", "error": "scripted error"})];
    let (_, _, chain) = serve(node);

    let events: Vec<WatchEvent> = block_on(
        block_on(chain.submit_and_watch(GENESIS, &extrinsic(1)))
            .unwrap()
            .collect(),
    );

    assert_eq!(events, vec![WatchEvent::Error("scripted error".into())]);
}

#[test]
fn head_events_need_no_runtime_metadata() {
    // Heads are hashed with Blake2-256 like extrinsics, so watching heads
    // costs no metadata download.
    let (node, provider, chain) = serve(Node::new(3, 5));
    let events = block_on(chain.head_events(GENESIS)).unwrap();
    let received = collect_in_background(events, 1);
    wait_for_subscriptions(&provider);
    let best = node.lock().unwrap().headers[5].clone();
    notification_sender(&provider)
        .unbounded_send(head_notification(BEST_SUBSCRIPTION, &best))
        .unwrap();

    let received = received.recv_timeout(TIMEOUT).expect("a head event");

    assert_eq!(
        received,
        vec![HeadEvent::Best(node.lock().unwrap().block(5))]
    );
    assert_eq!(method_count(&provider, "Metadata_metadata_at_version"), 0);
}
