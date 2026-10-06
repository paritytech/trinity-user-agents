use super::*;
use parity_scale_codec::DecodeAll;
use truapi::latest::DerivationIndex;

use crate::chain_runtime::ChainRuntime;
use crate::host_logic::product_account::{
    derivation_index_bytes, derive_product_keypair, derive_root_keypair_from_entropy,
};
use crate::runtime::bulletin_rpc::BulletinRpc;

const OBSERVED_RESOURCE_CHAIN: [u8; 32] = [0xcc; 32];
const ASSET_HUB_METADATA: &[u8] =
    include_bytes!("../../../../../../tests/fixtures/paseo-next-asset-hub-metadata.scale");
const RING_ROOTS: &[u8] =
    include_bytes!("../../../../../../tests/fixtures/paseo-next-asset-hub-ring-5-roots.scale");

fn answer(request: &serde_json::Value, result: serde_json::Value) -> Vec<String> {
    let mut frames =
        vec![json!({"jsonrpc": "2.0", "id": request["id"], "result": result}).to_string()];
    if request["method"] == "author_submitAndWatchExtrinsic" {
        frames.push(
            json!({
                "jsonrpc": "2.0", "method": "author_extrinsicUpdate",
                "params": {"subscription": "claim", "result": {"inBlock": "0xb10c"}},
            })
            .to_string(),
        );
    }
    frames
}

async fn people_provider(bulletin: bool) -> Arc<ScriptedProvider> {
    let member = crate::runtime::vrf::load()
        .await
        .unwrap()
        .member(&derive_lite_person_ring_vrf_entropy(&ENTROPY, "paseo"))
        .unwrap();
    let mut captured = RING_ROOTS;
    let roots = Vec::<([u8; 288], u32, u64, u64)>::decode_all(&mut captured).unwrap();
    let (root, revision, _, _) = roots[0];
    assert_eq!(revision, 105);
    let mut storage = VecDeque::from([
        json!(null),
        json!("0x000001043e000900"),
        encoded(5u32),
        encoded(vec![member]),
        json!(null),
        json!(null),
        encoded((root, revision, [0u8; 848])),
    ]);
    if bulletin {
        storage.push_front(encoded(b"paseo".to_vec()));
        storage.push_back(json!(null));
    }
    let storage = Mutex::new(storage);
    Arc::new(ScriptedProvider::with_frames(move |request| {
        let request: serde_json::Value = serde_json::from_str(request).unwrap();
        let result = match request["method"].as_str().unwrap() {
            "state_getRuntimeVersion" => json!({"specVersion": 1_000_000, "transactionVersion": 1}),
            "chain_getBlockHash" => json!(format!("0x{}", hex::encode([0xab; 32]))),
            "state_call" => match request["params"][0].as_str().unwrap() {
                "Metadata_metadata_at_version" => encoded(Some(PEOPLE_METADATA.to_vec())),
                "RuntimeViewFunction_execute_view_function" if bulletin => {
                    encoded(Ok::<Vec<u8>, ()>(10u8.encode()))
                }
                other => panic!("unexpected People runtime call: {other}"),
            },
            "chain_getFinalizedHead" => json!("0xfinal"),
            "state_getStorage" => storage.lock().unwrap().pop_front().unwrap(),
            "author_submitAndWatchExtrinsic" if bulletin => json!("claim"),
            "author_unwatchExtrinsic" if bulletin => json!(true),
            other => panic!("unexpected People RPC method: {other}"),
        };
        answer(&request, result)
    }))
}

async fn wallet(
    people: &ScriptedProvider,
    resource: Arc<ScriptedProvider>,
) -> (WalletAccountHolder, Db) {
    let platform = Arc::new(StubPlatform {
        rpc_connection: Mutex::new(Some(people.connection())),
        ..Default::default()
    });
    let mut services = RuntimeServices::new(
        platform,
        HostInfo {
            name: "resource journal test".to_string(),
            icon: None,
            version: None,
            platform: HostPlatform::Unknown,
        },
        [0x22; 32],
        [0xbb; 32],
        [0xaa; 32],
        test_spawner(),
    );
    let construction = Arc::get_mut(&mut services).unwrap();
    construction.chain = ChainRuntime::new(resource, test_spawner());
    construction.bulletin = BulletinRpc::new(construction.chain.clone(), [0xbb; 32]);
    super::wallet(services).await
}

async fn assert_record(
    database: &Db,
    owner: [u8; 32],
    resource: &str,
    account: [u8; 32],
    period: Option<u32>,
) {
    let records = rows(database, "SELECT * FROM allowance_records").await;
    assert_eq!(records.len(), 1);
    let allocated_at = records[0][4].clone();
    assert!(matches!(allocated_at, Value::Integer(time) if time > 1_000_000_000_000));
    assert_eq!(
        (
            records,
            rows(database, "SELECT * FROM statement_slots").await
        ),
        (
            vec![vec![
                Value::Blob(owner.to_vec()),
                Value::Blob(OBSERVED_RESOURCE_CHAIN.to_vec()),
                Value::Text(resource.to_string()),
                Value::Blob(account.to_vec()),
                allocated_at,
                Value::Integer(0),
                period.map_or(Value::Null, |period| Value::Integer(i64::from(period)))
            ]],
            Vec::<Vec<Value>>::new(),
        )
    );
}

#[test]
fn bulletin_is_journaled_only_after_its_authorization_becomes_visible() {
    futures::executor::block_on(bounded(async {
        let people = people_provider(true).await;
        let (visibility, pending_visibility) = oneshot::channel();
        let visibility = Mutex::new(Some(visibility));
        let reads = Mutex::new(0);
        let bulletin = Arc::new(ScriptedProvider::with_frames(move |request| {
            let request: serde_json::Value = serde_json::from_str(request).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "chain_getBlockHash" => {
                    json!(format!("0x{}", hex::encode(OBSERVED_RESOURCE_CHAIN)))
                }
                "chain_getHeader" => json!({"number": "0x1"}),
                "state_getStorage" => {
                    let mut reads = reads.lock().unwrap();
                    *reads += 1;
                    if *reads == 1 {
                        json!(null)
                    } else {
                        let response = answer(
                            &request,
                            encoded((0u32, 10u32, 0u64, 0u64, 1024u64, 1000u32)),
                        );
                        visibility
                            .lock()
                            .unwrap()
                            .take()
                            .unwrap()
                            .send(response[0].clone())
                            .unwrap();
                        return Vec::new();
                    }
                }
                other => panic!("unexpected Bulletin RPC method: {other}"),
            };
            answer(&request, result)
        }));
        let (wallet, database) = wallet(&people, bulletin.clone()).await;
        let session = wallet.current_session().unwrap();
        let allocate = wallet.allocate_bulletin_allowance(
            &session,
            PRODUCT,
            OnExistingAllowancePolicy::Increase,
        );
        let make_visible = async {
            let response = pending_visibility.await.unwrap();
            assert_eq!(
                (
                    rows(&database, "SELECT * FROM allowance_records").await,
                    rows(&database, "SELECT * FROM statement_slots").await,
                    submission_count(&people)
                ),
                (Vec::<Vec<Value>>::new(), Vec::<Vec<Value>>::new(), 1)
            );
            notification_sender(&bulletin)
                .unbounded_send(response)
                .unwrap();
        };
        let (result, ()) = futures::join!(allocate, make_visible);
        let expected =
            derive_sr25519_hard_path(&ENTROPY, &["allowance", "bulletin", PRODUCT]).unwrap();
        assert_eq!(result.unwrap(), expected.secret.to_bytes().to_vec());
        assert_record(
            &database,
            session.public_key,
            "bulletin-allowance",
            expected.public.to_bytes(),
            None,
        )
        .await;
        assert_eq!(
            (submission_count(&people), submission_count(&bulletin)),
            (1, 0)
        );
    }));
}

#[test]
fn pgas_journal_uses_the_observed_asset_hub_and_its_confirmed_day() {
    futures::executor::block_on(bounded(async {
        let people = people_provider(false).await;
        let storage = Mutex::new(VecDeque::from([
            encoded(b"paseo".to_vec()),
            encoded(7u64 * 86_400_000),
            encoded(0u32),
            json!(format!("0x{}", hex::encode(RING_ROOTS))),
            json!("0x"),
        ]));
        let asset_hub = Arc::new(ScriptedProvider::with_frames(move |request| {
            let request: serde_json::Value = serde_json::from_str(request).unwrap();
            let method = request["method"].as_str().unwrap();
            let result = match method {
                "state_getRuntimeVersion" => {
                    json!({"specVersion": 3_000_000, "transactionVersion": 1})
                }
                "chain_getBlockHash" => {
                    json!(format!("0x{}", hex::encode(OBSERVED_RESOURCE_CHAIN)))
                }
                "state_call" => {
                    assert_eq!(request["params"][0], "Metadata_metadata_at_version");
                    encoded(Some(ASSET_HUB_METADATA.to_vec()))
                }
                "state_getStorage" => storage.lock().unwrap().pop_front().unwrap(),
                "state_queryStorageAt" => json!([{"block": "0xb10c", "changes": []}]),
                "author_submitAndWatchExtrinsic" => json!("claim"),
                "author_unwatchExtrinsic" => json!(true),
                other => panic!("unexpected Asset Hub RPC method: {other}"),
            };
            if method == "state_getStorage" && storage.lock().unwrap().is_empty() {
                assert_eq!(request["params"][1], "0xb10c");
            }
            answer(&request, result)
        }));
        let (wallet, database) = wallet(&people, asset_hub.clone()).await;
        let session = wallet.current_session().unwrap();
        let index = DerivationIndex::Index(3);
        let expected = derive_product_keypair(
            &derive_root_keypair_from_entropy(&ENTROPY).unwrap(),
            PRODUCT,
            derivation_index_bytes(&index),
        )
        .unwrap();
        wallet
            .allocate_smart_contract_allowance(
                &session,
                PRODUCT,
                index,
                OnExistingAllowancePolicy::Increase,
            )
            .await
            .unwrap();
        assert_record(
            &database,
            session.public_key,
            "smart-contract-allowance",
            expected.public.to_bytes(),
            Some(7),
        )
        .await;
        assert_eq!(
            (submission_count(&people), submission_count(&asset_hub)),
            (0, 1)
        );
    }));
}
