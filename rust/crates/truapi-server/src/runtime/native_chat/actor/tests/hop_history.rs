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
fn nested_history_pages_survive_reopen_until_product_prepares_native_ack() {
    block_on(async {
        let pool = Pool::default();
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("history fixture has no Coinage or Chat RPC"),
            hop_provider: Some(Arc::new(pool.clone())),
            ..Default::default()
        });
        let fixture = Fixture::on_platform(platform.clone());
        set_product_grants(
            &platform,
            PRODUCT,
            PermissionAuthorizationStatus::Authorized,
        )
        .await;
        let actor = fixture.actor().await;
        let identity = IdentityFixture::new();
        let peer = DeviceFixture::new(1);
        seed_peer(&actor, &identity, &[&peer]).await;
        let registry = NativeChatRegistry::default();
        let wallet = rust_wallet(&registry, &fixture.context).await;
        let first = memo();
        let second = TransferMemo {
            entries: vec![MemoEntry(keypair(0x33).secret.to_bytes())],
            total_value: 80,
        };
        let before =
            wire::encode_text_message("before", fixture.timestamp, "Before history").unwrap();
        let after = wire::encode_text_message("after", fixture.timestamp, "After history").unwrap();
        let first_payment = encoded_payment("first-payment", fixture.timestamp, &first);
        let second_payment = encoded_payment("second-payment", fixture.timestamp, &second);
        // The small frames force the 256-frame limit independently of the byte
        // limit. The later large frames force multiple 256-KiB pages.
        let mut child_messages: Vec<_> = (0..300)
            .map(|index| {
                wire::encode_text_message(
                    &format!("small-{index}"),
                    fixture.timestamp,
                    "Nested history",
                )
                .unwrap()
            })
            .collect();
        child_messages.insert(128, second_payment.clone());
        let child_ticket = FileTicket::from_bytes(&[0xac; 32]).unwrap();
        let child = pool
            .compact("child", fixture.timestamp, &child_ticket, &child_messages)
            .await;
        let unauthorized_device = DeviceFixture::new(9);
        let historical_control = wire::encode_device_added_message(
            "old-device",
            fixture.timestamp,
            &unauthorized_device.account(),
            &unauthorized_device.public_key(),
        )
        .unwrap();
        let large_messages: Vec<_> = (0..70)
            .map(|index| {
                wire::encode_text_message(
                    &format!("large-{index}"),
                    fixture.timestamp,
                    &"x".repeat(8 * 1024),
                )
                .unwrap()
            })
            .collect();
        let mut root_messages = vec![
            before.clone(),
            child,
            first_payment.clone(),
            historical_control,
        ];
        root_messages.extend(large_messages.clone());
        root_messages.push(after.clone());
        let root_ticket = FileTicket::from_bytes(&[0xab; 32]).unwrap();
        let compacted = pool
            .compact("root", fixture.timestamp, &root_ticket, &root_messages)
            .await;
        let mut expected = vec![before];
        expected.extend(child_messages);
        expected.push(first_payment);
        expected.extend(large_messages);
        expected.push(after);
        assert!(expected.iter().map(Vec::len).sum::<usize>() > 256 * 1024);
        let packet = request(
            &actor,
            &identity,
            &peer,
            "history-request",
            std::slice::from_ref(&compacted),
        );
        let first_page = actor
            .open_statement(&fixture.context, &registry, packet.clone())
            .await
            .unwrap();
        let open_id = first_page.1.as_ref().unwrap().open_id;
        assert_eq!(
            actor
                .open_statement(&fixture.context, &registry, packet.clone())
                .await
                .unwrap(),
            first_page,
        );
        let mut pages = Vec::new();
        let mut recovered = Vec::new();
        let mut cursor = 0;
        loop {
            let page = actor
                .continue_open(&fixture.context, open_id, cursor)
                .await
                .unwrap();
            assert_eq!(
                actor
                    .continue_open(&fixture.context, open_id, cursor)
                    .await
                    .unwrap(),
                page,
            );
            assert_eq!(page.0.len(), 1);
            let opened = &page.0[0];
            assert_eq!(opened.peer_identity, identity.account);
            assert_eq!(opened.sender_account_id, peer.account());
            assert_eq!(opened.route, HostNativeChatRoute::Device);
            assert!(opened.plaintext.len() <= 256 * 1024);
            let wire::V2StatementTransportData::Request {
                request_id,
                messages,
            } = wire::decode_transport_plaintext(&opened.plaintext).unwrap()
            else {
                panic!("history page is not a native request")
            };
            assert_eq!(request_id, "history-request");
            assert!(messages.len() <= 256);
            if cursor == 0 {
                assert_eq!(messages.len(), 256);
            }
            recovered.extend(messages);
            let metadata = page.1.as_ref().unwrap();
            assert_eq!(metadata.open_id, open_id);
            assert_eq!(metadata.cursor, cursor);
            let next = metadata.next_cursor;
            pages.push(page);
            let Some(next) = next else { break };
            cursor = next;
        }
        // Exact frames include both incoming bearer memos, in their original
        // positions. Historical device control cannot grant live authority.
        assert_eq!(recovered, expected);
        assert!(pages.len() >= 3);
        assert_eq!(pool.0.claims.load(Ordering::SeqCst), 2);
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 0);
        assert_eq!(pool.0.entries.lock().len(), 2);
        assert!(wallet.views(PRODUCT).await.unwrap().is_empty());
        let view = actor.public_view(&fixture.context, vec![]).await.unwrap();
        assert!(view.prepared.is_empty());
        assert!(view.migration.is_none());
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
            1,
        );
        let unauthorized = request(
            &actor,
            &identity,
            &unauthorized_device,
            "historical-authority",
            &[wire::encode_text_message("forged", fixture.timestamp, "not admitted").unwrap()],
        );
        assert_eq!(
            actor
                .open_statement(&fixture.context, &registry, unauthorized)
                .await,
            Err(Error::InvalidStatement),
        );
        fixture.tasks.stop();
        drop(wallet);
        drop(actor);
        drop(registry);
        let restarted = Fixture::on_platform(platform);
        let actor = restarted.actor().await;
        let registry = NativeChatRegistry::default();
        assert_eq!(
            actor
                .open_statement(&restarted.context, &registry, packet)
                .await
                .unwrap(),
            first_page,
        );
        for page in &pages {
            assert_eq!(
                actor
                    .continue_open(&restarted.context, open_id, page.1.as_ref().unwrap().cursor,)
                    .await
                    .unwrap(),
                *page,
            );
        }
        assert_eq!(pool.0.claims.load(Ordering::SeqCst), 2);
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 0);
        assert!(
            registry
                .wallet(&restarted.context)
                .await
                .unwrap()
                .views(&restarted.context, PRODUCT)
                .await
                .unwrap()
                .is_empty()
        );
        // A rejection is not a successful native ACK and cannot reclaim HOP.
        actor
            .prepare(
                &restarted.context,
                identity.account,
                HostNativeChatRoute::Device,
                wire::encode_transport_response_plaintext("history-request", 1).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 0);
        assert_eq!(
            actor
                .continue_open(&restarted.context, open_id, 0)
                .await
                .unwrap(),
            first_page,
        );
        let prepared = actor
            .prepare(
                &restarted.context,
                identity.account,
                HostNativeChatRoute::Device,
                wire::encode_transport_response_plaintext("history-request", 0).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].peer_identity, identity.account);
        assert_eq!(prepared[0].request_id, "history-request");
        assert!(!prepared[0].requires_ack);
        let wire::V2StatementTransportData::MultiResponse(multi) =
            open_output(&actor, &identity, &prepared[0].statement, true, false)
        else {
            panic!("not a native multi response")
        };
        let body = open_body(
            &actor,
            &peer,
            &multi.encrypted_response,
            &multi.devices_info,
        );
        assert_eq!(
            wire::decode_message_exchange_response_plaintext(&body).unwrap(),
            wire::V2MessageExchangeResponse {
                request_id: "history-request".into(),
                response_code: 0
            },
        );
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 2);
        assert!(pool.0.entries.lock().is_empty());
        assert_eq!(
            actor.continue_open(&restarted.context, open_id, 0).await,
            Err(Error::OperationNotFound),
        );
        // A refreshed statement cannot fetch or re-ACK a deleted HOP entry.
        let replay = request(
            &actor,
            &identity,
            &peer,
            "refreshed-history-request",
            &[compacted],
        );
        let (opened, continuation) = actor
            .open_statement(&restarted.context, &registry, replay)
            .await
            .unwrap();
        assert!(continuation.is_none());
        assert_eq!(
            opened[0].plaintext,
            wire::encode_transport_request_plaintext("refreshed-history-request", &[]).unwrap(),
        );
        assert_eq!(pool.0.claims.load(Ordering::SeqCst), 2);
        assert_eq!(pool.0.acknowledgments.load(Ordering::SeqCst), 2);
        assert!(
            actor
                .public_view(&restarted.context, vec![])
                .await
                .unwrap()
                .prepared
                .is_empty()
        );
    });
}
