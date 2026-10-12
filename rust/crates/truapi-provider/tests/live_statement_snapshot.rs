//! Live check that a light-client statement subscription starts with the
//! statements the network already stores, excluded from CI. Run manually with:
//!
//! ```text
//! cargo test -p truapi-provider --features networks --test live_statement_snapshot -- --ignored --nocapture
//! ```
//!
//! The test picks a topic that Paseo Next V2 People currently stores
//! statements under, reads them from a full node over JSON-RPC, then expects
//! the embedded light client to return every one of them in the initial pages
//! of the same `statement_subscribeStatement`, on two connections in turn.
//! `STATEMENT_SNAPSHOT_TOPIC` pins the topic; `PEOPLE_RPC_URL` the full node.

#![cfg(all(feature = "ws", feature = "networks", not(target_arch = "wasm32")))]

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::stream::{BoxStream, StreamExt};
use serde_json::{Value, json};
use truapi_provider::platform::{ChainProvider, JsonRpcConnection};
use truapi_provider::{ChainPhase, ChainSource, EmbeddedChainProvider};

const PEOPLE_RPC_URL: &str = "wss://paseo-people-next-system-rpc.polkadot.io";
/// Registry key for the full node; distinct from every catalog genesis.
const RPC_KEY: [u8; 32] = [0xee; 32];
/// Statements whose expiry is closer than this are not used, so none expires
/// between the two reads being compared.
const MIN_LIFETIME_SECS: u64 = 300;

/// Subscribe `filter` on `connection` and return the subscription id.
async fn subscribe(
    connection: &dyn JsonRpcConnection,
    responses: &mut BoxStream<'static, String>,
    id: u64,
    filter: Value,
) -> String {
    connection.send(
        json!({"jsonrpc":"2.0","id":id,"method":"statement_subscribeStatement","params":[filter]})
            .to_string(),
    );
    loop {
        let frame: Value =
            serde_json::from_str(&responses.next().await.expect("the connection stays alive"))
                .expect("frames are valid JSON");
        if frame["id"] == id {
            return frame["result"]
                .as_str()
                .unwrap_or_else(|| panic!("subscription refused: {frame}"))
                .to_owned();
        }
    }
}

/// Collect the initial pages of `subscription`: every page up to the first
/// one whose `remaining` is zero or absent, as truapi's Media lookup reads it.
async fn initial_pages(
    responses: &mut BoxStream<'static, String>,
    subscription: &str,
) -> Vec<String> {
    let mut statements = Vec::new();
    loop {
        let frame: Value =
            serde_json::from_str(&responses.next().await.expect("the connection stays alive"))
                .expect("frames are valid JSON");
        if frame["method"] != "statement_statement"
            || frame["params"]["subscription"] != subscription
        {
            continue;
        }
        let data = &frame["params"]["result"]["data"];
        statements.extend(
            data["statements"]
                .as_array()
                .expect("a page carries statements")
                .iter()
                .map(|statement| statement.as_str().expect("hex statement").to_owned()),
        );
        if data["remaining"].as_u64().unwrap_or(0) == 0 {
            return statements;
        }
    }
}

/// Topics and expiry (UNIX seconds) of a SCALE-encoded statement, or `None`
/// for an encoding this reader does not follow.
fn topics_and_expiry(encoded: &[u8]) -> Option<(Vec<[u8; 32]>, u64)> {
    fn take<'a>(input: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
        (input.len() >= len).then(|| {
            let (head, tail) = input.split_at(len);
            *input = tail;
            head
        })
    }
    fn compact(input: &mut &[u8]) -> Option<u64> {
        let first = *input.first()?;
        Some(match first & 0b11 {
            0 => u64::from(take(input, 1)?[0] >> 2),
            1 => u64::from(u16::from_le_bytes(take(input, 2)?.try_into().ok()?) >> 2),
            2 => u64::from(u32::from_le_bytes(take(input, 4)?.try_into().ok()?) >> 2),
            _ => return None,
        })
    }
    let mut input = encoded;
    let fields = compact(&mut input)?;
    let mut topics = Vec::new();
    let mut expiry = None;
    for _ in 0..fields {
        match take(&mut input, 1)?[0] {
            // Proof: sr25519 and ed25519 (64 + 32), secp256k1 (65 + 33).
            0 => match take(&mut input, 1)?[0] {
                0 | 1 => {
                    take(&mut input, 96)?;
                }
                2 => {
                    take(&mut input, 98)?;
                }
                _ => return None,
            },
            1 | 3 => {
                take(&mut input, 32)?;
            }
            2 => expiry = Some(u64::from_le_bytes(take(&mut input, 8)?.try_into().ok()?) >> 32),
            4..=7 => topics.push(take(&mut input, 32)?.try_into().ok()?),
            8 => {
                let len = usize::try_from(compact(&mut input)?).ok()?;
                take(&mut input, len)?;
            }
            _ => return None,
        }
    }
    Some((topics, expiry?))
}

/// A topic the full node stores statements under, all of them long-lived.
async fn stored_topic(
    connection: &dyn JsonRpcConnection,
    responses: &mut BoxStream<'static, String>,
) -> String {
    if let Ok(topic) = std::env::var("STATEMENT_SNAPSHOT_TOPIC") {
        return topic;
    }
    let subscription = subscribe(connection, responses, 1, json!("any")).await;
    let store = initial_pages(responses, &subscription).await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs();
    // Per first topic: how many statements, and whether any expires soon.
    let mut topics: HashMap<[u8; 32], (usize, bool)> = HashMap::new();
    for statement in &store {
        let bytes = hex::decode(statement.trim_start_matches("0x")).expect("hex statement");
        let Some((statement_topics, expiry)) = topics_and_expiry(&bytes) else {
            continue;
        };
        let Some(topic) = statement_topics.first() else {
            continue;
        };
        let entry = topics.entry(*topic).or_default();
        entry.0 += 1;
        entry.1 |= expiry < now + MIN_LIFETIME_SECS;
    }
    let (topic, (count, _)) = topics
        .into_iter()
        .filter(|(_, (count, expiring))| !expiring && *count <= 64)
        .max_by_key(|(_, (count, _))| *count)
        .expect("the full node stores a long-lived statement under some topic");
    println!(
        "picked topic 0x{} ({count} statements of {} stored)",
        hex::encode(topic),
        store.len()
    );
    format!("0x{}", hex::encode(topic))
}

#[tokio::test]
#[ignore = "requires network access to Paseo Next V2 People"]
async fn light_client_initial_page_carries_stored_statements() {
    let url = std::env::var("PEOPLE_RPC_URL").unwrap_or_else(|_| PEOPLE_RPC_URL.to_owned());
    let (builder, chains) = EmbeddedChainProvider::builder()
        .chain(
            RPC_KEY,
            ChainSource::rpc_node(url::Url::parse(&url).expect("valid URL")),
        )
        .add_network("paseo-next-v2")
        .expect("the catalog has paseo-next-v2");
    let provider = builder.build();

    let full_node = provider.connect(RPC_KEY).await.expect("full node connects");
    let mut full_node_responses = full_node.responses();
    let topic = stored_topic(full_node.as_ref(), &mut full_node_responses).await;
    let filter = json!({ "matchAll": [topic] });
    let subscription = subscribe(
        full_node.as_ref(),
        &mut full_node_responses,
        2,
        filter.clone(),
    )
    .await;
    let stored: BTreeSet<String> = initial_pages(&mut full_node_responses, &subscription)
        .await
        .into_iter()
        .collect();
    println!("full node: {} statements under {topic}", stored.len());
    assert!(
        !stored.is_empty(),
        "the full node stores nothing under {topic}"
    );

    let first = provider
        .connect(chains.people)
        .await
        .expect("People connects");
    let mut lifecycle = provider
        .lifecycle(chains.people)
        .expect("a connected chain has a lifecycle");
    tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            let state = lifecycle.next().await.expect("the chain stays running");
            if state.phase == ChainPhase::Ready && state.peers > 0 {
                return;
            }
        }
    })
    .await
    .expect("People reaches ready with peers");

    let second = provider
        .connect(chains.people)
        .await
        .expect("People connects");
    for (name, connection) in [("first", &first), ("second", &second)] {
        let mut responses = connection.responses();
        let started = Instant::now();
        let subscription = subscribe(connection.as_ref(), &mut responses, 1, filter.clone()).await;
        let initial: BTreeSet<String> = tokio::time::timeout(
            Duration::from_secs(30),
            initial_pages(&mut responses, &subscription),
        )
        .await
        .expect("the initial pages end in time")
        .into_iter()
        .collect();
        println!(
            "{name} light connection: {} statements in the initial pages after {:?}",
            initial.len(),
            started.elapsed()
        );
        let missing: BTreeSet<String> = stored.difference(&initial).cloned().collect();
        if !missing.is_empty() {
            // Diagnose: do the peers still deliver them, only too late?
            let late = late_arrivals(&mut responses, &subscription, &missing, started).await;
            panic!(
                "{name} light connection's initial pages miss {} of the {} statements the full \
                 node stores; afterwards, as live notifications: {late:?}",
                missing.len(),
                stored.len()
            );
        }
    }
}

/// When each live notification carrying `missing` statements arrived, until
/// all of them did or six seconds passed.
async fn late_arrivals(
    responses: &mut BoxStream<'static, String>,
    subscription: &str,
    missing: &BTreeSet<String>,
    started: Instant,
) -> Vec<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    let mut arrivals = Vec::new();
    let mut outstanding = missing.clone();
    while !outstanding.is_empty() {
        let Ok(frame) = tokio::time::timeout_at(deadline, responses.next()).await else {
            arrivals.push(format!("{} never arrived", outstanding.len()));
            break;
        };
        let frame: Value = serde_json::from_str(&frame.expect("the connection stays alive"))
            .expect("frames are valid JSON");
        if frame["params"]["subscription"] != subscription {
            continue;
        }
        let statements = frame["params"]["result"]["data"]["statements"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|statement| outstanding.remove(*statement))
            .count();
        arrivals.push(format!("{statements} after {:?}", started.elapsed()));
    }
    arrivals
}
