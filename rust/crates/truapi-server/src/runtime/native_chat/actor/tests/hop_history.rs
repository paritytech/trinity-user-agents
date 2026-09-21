// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
mod attachments;
use crate::runtime::native_chat::hop::{
    self, FileTicket, HopClient, HopError, HopRpc, MultiSignature, MultiSigner, PreparedUpload,
    SenderProof, SenderProofProviding,
};
use futures::{StreamExt, channel::mpsc, stream::BoxStream};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::atomic::AtomicUsize};
use truapi::latest::GenericError;
use truapi_platform::{HopProvider, JsonRpcConnection, PermissionAuthorizationStatus};

const ENDPOINT: &str = "wss://history.fixture.invalid";

#[derive(Default)]
struct PoolState {
    entries: Mutex<BTreeMap<String, String>>,
    claims: AtomicUsize,
    acknowledgments: AtomicUsize,
    submissions: Mutex<Vec<String>>,
    reject_next_submit: AtomicBool,
    expected_sender: Mutex<Option<[u8; 32]>>,
}
#[derive(Clone, Default)]
struct Pool(Arc<PoolState>);

#[async_trait::async_trait]
impl HopRpc for Pool {
    async fn call(&self, method: &str, params: Value) -> Result<Value, HopError> {
        assert_eq!(method, "hop_submit");
        let data = params["data"].as_str().unwrap().to_owned();
        let encrypted = hex::decode(data.strip_prefix("0x").unwrap()).unwrap();
        let hash = format!("0x{}", hex::encode(hop::blake2b_256(&encrypted)));
        self.0.entries.lock().insert(hash, data);
        Ok(
            json!({"poolStatus":{"entryCount":self.0.entries.lock().len(),"totalBytes":encrypted.len(),"maxBytes":4000000}}),
        )
    }
}

struct Sender;
#[async_trait::async_trait]
impl SenderProofProviding for Sender {
    async fn proof(&self, hash: &[u8; 32]) -> Result<SenderProof, HopError> {
        let signer = keypair(0x61);
        let submit_timestamp = 1770000000000;
        Ok(SenderProof {
            sender: MultiSigner::Sr25519(signer.public.to_bytes()),
            signature: MultiSignature::Sr25519(
                signer
                    .sign_simple(
                        b"substrate",
                        &hop::sender_proof_payload(hash, submit_timestamp),
                    )
                    .to_bytes(),
            ),
            submit_timestamp,
        })
    }
}

impl Pool {
    fn submit(&self, params: &Value) -> Value {
        let data = params["data"].as_str().unwrap().to_owned();
        let encrypted = hex::decode(data.trim_start_matches("0x")).unwrap();
        let hash = hop::blake2b_256(&encrypted);
        let signer =
            hex::decode(params["signer"].as_str().unwrap().trim_start_matches("0x")).unwrap();
        let signature = hex::decode(
            params["signature"]
                .as_str()
                .unwrap()
                .trim_start_matches("0x"),
        )
        .unwrap();
        assert_eq!(signer[0], 1);
        assert_eq!(signature[0], 1);
        let public = schnorrkel::PublicKey::from_bytes(&signer[1..]).unwrap();
        if let Some(expected) = *self.0.expected_sender.lock() {
            assert_eq!(public.to_bytes(), expected);
        }
        public
            .verify_simple(
                b"substrate",
                &hop::sender_proof_payload(&hash, params["submit_timestamp"].as_u64().unwrap()),
                &schnorrkel::Signature::from_bytes(&signature[1..]).unwrap(),
            )
            .unwrap();
        let hash = format!("0x{}", hex::encode(hash));
        self.0.submissions.lock().push(hash.clone());
        self.0.entries.lock().insert(hash, data);
        json!({"poolStatus":{"entryCount":self.0.entries.lock().len(),"totalBytes":encrypted.len(),"maxBytes":16000000}})
    }
    async fn compact(
        &self,
        id: &str,
        timestamp: u64,
        ticket: &FileTicket,
        messages: &[Vec<u8>],
    ) -> Vec<u8> {
        let prepared = PreparedUpload::compaction(messages, ticket).unwrap();
        let submitted = HopClient::new(self)
            .submit(&prepared, &Sender)
            .await
            .unwrap();
        wire::encode_compacted_messages_message(
            id,
            timestamp,
            &submitted.hash,
            ticket.as_bytes(),
            &wire::V2NodeEndpoint::WssUrl(ENDPOINT.into()),
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
impl HopProvider for Pool {
    async fn allowed_hop_endpoints(&self, genesis: [u8; 32]) -> Result<Vec<String>, GenericError> {
        assert_eq!(genesis, [3; 32]);
        Ok(vec![ENDPOINT.into()])
    }
    async fn connect_hop(
        &self,
        genesis: [u8; 32],
        endpoint: String,
    ) -> Result<Box<dyn JsonRpcConnection>, GenericError> {
        assert_eq!(genesis, [3; 32]);
        assert_eq!(endpoint, ENDPOINT);
        let (sender, receiver) = mpsc::unbounded();
        Ok(Box::new(Connection {
            pool: self.clone(),
            sender: Mutex::new(Some(sender)),
            receiver: Mutex::new(Some(receiver)),
        }))
    }
}

struct Connection {
    pool: Pool,
    sender: Mutex<Option<mpsc::UnboundedSender<String>>>,
    receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
}
impl JsonRpcConnection for Connection {
    fn send(&self, request: String) {
        let request: Value = serde_json::from_str(&request).unwrap();
        // HOP uses named parameters, unlike the bitswap array parameters.
        let hash = request["params"]["raw_hash"].as_str().unwrap_or("");
        let result = match request["method"].as_str().unwrap() {
            "hop_submit" => {
                let result = self.pool.submit(&request["params"]);
                if self.pool.0.reject_next_submit.swap(false, Ordering::SeqCst) {
                    let response = json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32001,"message":"accepted but response lost"}});
                    self.sender
                        .lock()
                        .as_ref()
                        .unwrap()
                        .unbounded_send(response.to_string())
                        .unwrap();
                    return;
                }
                Some(result)
            }
            "hop_claim" => {
                self.pool.0.claims.fetch_add(1, Ordering::SeqCst);
                self.pool
                    .0
                    .entries
                    .lock()
                    .get(hash)
                    .cloned()
                    .map(Value::String)
            }
            "hop_ack" => {
                self.pool.0.acknowledgments.fetch_add(1, Ordering::SeqCst);
                self.pool.0.entries.lock().remove(hash);
                Some(Value::Null)
            }
            other => panic!("unexpected HOP fixture method {other}"),
        };
        let response = match result {
            Some(result) => json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
            None => {
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":1004,"message":"absent"}})
            }
        };
        if let Some(sender) = self.sender.lock().as_ref() {
            sender.unbounded_send(response.to_string()).unwrap();
        }
    }
    fn responses(&self) -> BoxStream<'static, String> {
        self.receiver.lock().take().unwrap().boxed()
    }
    fn close(&self) {
        self.sender.lock().take();
    }
}

fn encoded_payment(id: &str, timestamp: u64, memo: &TransferMemo) -> Vec<u8> {
    wire::encode_coinage_send_message(
        id,
        timestamp,
        &memo.total_value.to_string(),
        &memo
            .entries
            .iter()
            .map(|entry| entry.0.to_vec())
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

#[test]
fn nested_history_keeps_all_claim_plans_before_ack_and_survives_pool_deletion() {
    block_on(async {
        let pool = Pool::default();
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("history fixture has no Coinage RPC"),
            hop_provider: Some(Arc::new(pool.clone())),
            ..Default::default()
        });
        let fixture = Fixture::on_platform(platform.clone());
        set_background_grants(&platform, PermissionAuthorizationStatus::Authorized).await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = registry.wallet(&fixture.context).await.unwrap();
        let mut first = memo();
        first.total_value = 240;
        let second = TransferMemo {
            entries: vec![MemoEntry(keypair(0x33).secret.to_bytes())],
            total_value: 80,
        };
        wallet
            .seed_incoming_for_test(
                &fixture.context,
                PRODUCT,
                identity.account,
                "original-first",
                "first-payment",
                fixture.timestamp,
                &first,
                true,
            )
            .await
            .unwrap();
        wallet
            .seed_incoming_for_test(
                &fixture.context,
                PRODUCT,
                identity.account,
                "original-second",
                "second-payment",
                fixture.timestamp,
                &second,
                false,
            )
            .await
            .unwrap();
        let before =
            wire::encode_text_message("before", fixture.timestamp, "Before history").unwrap();
        let middle =
            wire::encode_text_message("middle", fixture.timestamp, "Nested history").unwrap();
        let after = wire::encode_text_message("after", fixture.timestamp, "After history").unwrap();
        let child_ticket = FileTicket::from_bytes(&[0xac; 32]).unwrap();
        let child = pool
            .compact(
                "child",
                fixture.timestamp,
                &child_ticket,
                &[
                    middle.clone(),
                    encoded_payment("second-payment", fixture.timestamp, &second),
                ],
            )
            .await;
        let unauthorized_device = DeviceFixture::new(9);
        let historical_control = wire::encode_device_added_message(
            "old-device",
            fixture.timestamp,
            &unauthorized_device.account(),
            &unauthorized_device.public_key(),
        )
        .unwrap();
        let root_ticket = FileTicket::from_bytes(&[0xab; 32]).unwrap();
        let compacted = pool
            .compact(
                "root",
                fixture.timestamp,
                &root_ticket,
                &[
                    before.clone(),
                    child,
                    encoded_payment("first-payment", fixture.timestamp, &first),
                    historical_control,
                    after.clone(),
                ],
            )
            .await;
        let packet = request(
            &actor,
            &identity,
            &peer,
            "history-request",
            &[compacted.clone()],
        );
        assert_eq!(
            actor
                .receive(&fixture.context, &registry, packet.clone())
                .await,
            Err(Error::NetworkUnavailable)
        );
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 0);
        assert!(
            actor
                .store
                .read(|state| state.outbox.is_empty() && state.messages.is_empty())
                .await
                .unwrap()
        );

        wallet
            .persist_incoming_plan_for_test(&second)
            .await
            .unwrap();
        // Statement delivery remains offline, but HOP custody now commits.
        assert_eq!(
            actor.receive(&fixture.context, &registry, packet).await,
            Err(Error::NetworkUnavailable)
        );
        actor.acknowledge_history(&fixture.context).await.unwrap();
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 2);
        assert!(pool.0.entries.lock().is_empty());
        let view = actor
            .public_view(&fixture.context, wallet.views(PRODUCT).await.unwrap())
            .await
            .unwrap();
        assert_eq!(view.messages[0].messages, vec![before, middle, after]);
        assert_eq!(view.payments.len(), 2);
        let public_bytes = view.encode();
        assert!(
            !public_bytes
                .windows(32)
                .any(|part| part == root_ticket.as_bytes() || part == child_ticket.as_bytes())
        );
        for memo in [&first, &second] {
            for source in &memo.entries {
                assert!(!public_bytes.windows(64).any(|part| part == source.0));
            }
        }
        assert_eq!(
            actor
                .store
                .read(|state| state
                    .peer(&identity.account)
                    .unwrap()
                    .active_devices()
                    .len())
                .await
                .unwrap(),
            1
        );
        let claims = pool.0.claims.load(Ordering::SeqCst);
        fixture.tasks.stop();
        drop(wallet);
        drop(actor);
        drop(registry);
        let restarted = Fixture::on_platform(platform);
        let actor = restarted.actor().await;
        let registry = NativeChatRegistry::default();
        // A refreshed outer request must not fetch or re-ACK a deleted HOP entry.
        let replay = request(
            &actor,
            &identity,
            &peer,
            "refreshed-history-request",
            &[compacted],
        );
        assert_eq!(
            actor.receive(&restarted.context, &registry, replay).await,
            Err(Error::NetworkUnavailable)
        );
        actor.acknowledge_history(&restarted.context).await.unwrap();
        assert_eq!(pool.0.claims.load(Ordering::SeqCst), claims);
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 2);
        assert_eq!(
            actor
                .public_view(&restarted.context, vec![])
                .await
                .unwrap()
                .messages,
            view.messages
        );
    });
}
