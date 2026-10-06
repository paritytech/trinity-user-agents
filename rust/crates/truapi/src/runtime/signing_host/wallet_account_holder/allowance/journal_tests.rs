mod resources;

use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use futures::FutureExt;
use futures::channel::oneshot;
use parity_scale_codec::Encode;
use rusqlite::types::Value;
use serde_json::json;
use truapi::latest::HostPlatform;

use crate::host_logic::product_account::{
    derive_lite_person_ring_vrf_entropy, derive_sr25519_hard_path,
};
use crate::platform::HostInfo;
use crate::runtime::RuntimeServices;
use crate::runtime::statement_allowance::slot;
use crate::store::{Db, DbConfig, DbLocation, core_migrations};
use crate::test_support::{ScriptedProvider, StubPlatform, notification_sender, test_spawner};

const ENTROPY: [u8; 32] = [7; 32];
const PRODUCT: &str = "journal.paseo";
const OBSERVED_CHAIN: [u8; 32] = [0xab; 32];
const PEOPLE_METADATA: &[u8] =
    include_bytes!("../../../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

struct Fixture {
    wallet: WalletAccountHolder,
    database: Db,
    provider: Arc<ScriptedProvider>,
    verification: Option<oneshot::Receiver<String>>,
    target: [u8; 32],
}

fn encoded(value: impl Encode) -> serde_json::Value {
    json!(format!("0x{}", hex::encode(value.encode())))
}

async fn fixture(pause_verification: bool) -> Fixture {
    let target = derive_sr25519_hard_path(&ENTROPY, &["allowance", "statement-store", PRODUCT])
        .unwrap()
        .public
        .to_bytes();
    let member = crate::runtime::vrf::load()
        .await
        .unwrap()
        .member(&derive_lite_person_ring_vrf_entropy(&ENTROPY, "paseo"))
        .unwrap();
    let storage = Mutex::new(VecDeque::from([
        encoded(b"paseo".to_vec()),
        json!(null),
        json!("0x000001043e000900"),
        json!(null),
        encoded(vec![member]),
        json!(null),
        json!(null),
        json!(null),
        encoded((target, 0u32, 1_000u64)),
    ]));
    let budgets = Mutex::new(VecDeque::from([20u32, 10u32]));
    let (verification, received_verification) = oneshot::channel();
    let verification = Mutex::new(Some(verification));
    let provider = Arc::new(ScriptedProvider::with_frames(move |request| {
        let request: serde_json::Value = serde_json::from_str(request).unwrap();
        let method = request["method"].as_str().unwrap();
        let result = match method {
            "state_getRuntimeVersion" => json!({"specVersion": 1_000_000, "transactionVersion": 1}),
            "chain_getBlockHash" => json!(format!("0x{}", hex::encode(OBSERVED_CHAIN))),
            "state_call" => match request["params"][0].as_str().unwrap() {
                "Metadata_metadata_at_version" => encoded(Some(PEOPLE_METADATA.to_vec())),
                "RuntimeViewFunction_execute_view_function" => encoded(Ok::<Vec<u8>, ()>(
                    budgets.lock().unwrap().pop_front().unwrap().encode(),
                )),
                other => panic!("unexpected runtime call: {other}"),
            },
            "state_queryStorageAt" => json!([{"block": "0xb10c", "changes": []}]),
            "chain_getFinalizedHead" => json!("0xfinal"),
            "state_getStorage" => storage.lock().unwrap().pop_front().unwrap(),
            "author_submitAndWatchExtrinsic" => json!("allocation-sub"),
            "author_unwatchExtrinsic" => json!(true),
            other => panic!("unexpected RPC method: {other}"),
        };
        let response = json!({"jsonrpc": "2.0", "id": request["id"], "result": result}).to_string();
        if method == "state_getStorage" && storage.lock().unwrap().is_empty() {
            assert_eq!(request["params"][1], "0xb10c");
            if pause_verification {
                verification
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap()
                    .send(response)
                    .unwrap();
                return Vec::new();
            }
        }
        let mut frames = vec![response];
        if method == "author_submitAndWatchExtrinsic" {
            frames.push(
                json!({
                    "jsonrpc": "2.0",
                    "method": "author_extrinsicUpdate",
                    "params": {"subscription": "allocation-sub", "result": {"inBlock": "0xb10c"}},
                })
                .to_string(),
            );
        }
        frames
    }));
    let platform = Arc::new(StubPlatform {
        rpc_connection: Mutex::new(Some(provider.connection())),
        ..Default::default()
    });
    let services = RuntimeServices::new(
        platform,
        HostInfo {
            name: "journal test".to_string(),
            icon: None,
            version: None,
            platform: HostPlatform::Unknown,
        },
        [0x22; 32],
        [0; 32],
        [0; 32],
        test_spawner(),
    );
    let (wallet, database) = wallet(services).await;
    Fixture {
        wallet,
        database,
        provider,
        verification: Some(received_verification),
        target,
    }
}

async fn wallet(services: Arc<RuntimeServices>) -> (WalletAccountHolder, Db) {
    let database = Db::open(DbConfig {
        location: DbLocation::Memory,
        migrations: core_migrations,
        readers: 1,
    })
    .await
    .unwrap();
    assert!(services.install_core_db(database.clone()));
    let registry = crate::runtime::RingVrfRegistryStore::new(services.platform.clone());
    let wallet = WalletAccountHolder::new(services, "paseo".to_string(), registry);
    wallet
        .install(wallet.prepare_activation(ENTROPY.to_vec(), None).unwrap())
        .unwrap();
    (wallet, database)
}

async fn rows(database: &Db, query: &'static str) -> Vec<Vec<Value>> {
    database
        .read(move |connection| {
            let mut statement = connection.prepare(query)?;
            let columns = statement.column_count();
            Ok(statement
                .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())?
                .collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .unwrap()
}

async fn assert_statement_journal(fixture: &Fixture, owner: [u8; 32], period: u32) {
    let accounts = rows(&fixture.database,
        "SELECT wallet, chain, resource, account, allocated_at, priority, last_renewed_period FROM allowance_records").await;
    let slots = rows(&fixture.database,
        "SELECT wallet, chain, collection, period, slot, account, priority, last_allocated_or_renewed_at FROM statement_slots").await;
    assert_eq!((accounts.len(), slots.len()), (1, 1));
    let allocated_at = accounts[0][4].clone();
    assert!(matches!(allocated_at, Value::Integer(time) if time > 1_000_000_000_000));
    let owner = Value::Blob(owner.to_vec());
    let chain = Value::Blob(OBSERVED_CHAIN.to_vec());
    let target = Value::Blob(fixture.target.to_vec());
    assert_eq!(
        (accounts, slots),
        (
            vec![vec![
                owner.clone(),
                chain.clone(),
                Value::Text("statement-store-allowance".to_string()),
                target.clone(),
                allocated_at.clone(),
                Value::Null,
                Value::Null,
            ]],
            vec![vec![
                owner,
                chain,
                Value::Text("LitePeople".to_string()),
                Value::Integer(i64::from(period)),
                Value::Integer(0),
                target,
                Value::Integer(0),
                allocated_at,
            ]],
        )
    );
    assert_eq!(submission_count(&fixture.provider), 1);
}

fn submission_count(provider: &ScriptedProvider) -> usize {
    provider
        .sent
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request.contains("author_submitAndWatchExtrinsic"))
        .count()
}

async fn bounded<T>(future: impl core::future::Future<Output = T>) -> T {
    let future = future.fuse();
    let timeout = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse();
    futures::pin_mut!(future, timeout);
    futures::select! {
        result = future => result,
        _ = timeout => panic!("wallet allocation did not finish its scripted RPC exchange"),
    }
}

#[test]
fn confirmed_statement_allocation_records_the_observed_chain_and_exact_slot() {
    futures::executor::block_on(bounded(async {
        let fixture = fixture(false).await;
        let session = fixture.wallet.current_session().unwrap();
        let allocation = fixture
            .wallet
            .allocate_statement_store_allowance(
                &session,
                PRODUCT,
                OnExistingAllowancePolicy::Increase,
            )
            .await
            .unwrap();
        let expected =
            derive_sr25519_hard_path(&ENTROPY, &["allowance", "statement-store", PRODUCT]).unwrap();
        assert_eq!(allocation.secret, expected.secret.to_bytes().to_vec());
        assert_statement_journal(&fixture, session.public_key, allocation.period).await;
    }));
}

#[test]
fn a_confirmed_allocation_stays_with_its_original_wallet_after_replacement() {
    futures::executor::block_on(bounded(async {
        let mut fixture = fixture(true).await;
        let verification = fixture.verification.take().unwrap();
        let session = fixture.wallet.current_session().unwrap();
        let period = slot::current_period(current_unix_secs().unwrap());
        let allocation = fixture.wallet.allocate_statement_store_allowance(
            &session,
            PRODUCT,
            OnExistingAllowancePolicy::Increase,
        );
        let replace_wallet = async {
            let response = verification.await.unwrap();
            fixture
                .wallet
                .install(
                    fixture
                        .wallet
                        .prepare_activation(vec![8; 32], None)
                        .unwrap(),
                )
                .unwrap();
            notification_sender(&fixture.provider)
                .unbounded_send(response)
                .unwrap();
        };
        let (result, ()) = futures::join!(allocation, replace_wallet);
        assert_eq!(
            result
                .map(|_| ())
                .map_err(AllowanceAllocationError::into_authority_error),
            Err(AuthorityError::Disconnected)
        );
        assert_statement_journal(&fixture, session.public_key, period).await;
    }));
}

#[test]
fn a_failed_slot_write_rolls_back_the_account_without_resubmitting() {
    futures::executor::block_on(bounded(async {
        let fixture = fixture(false).await;
        fixture
            .database
            .write(|transaction| {
                transaction.execute_batch(
                    "CREATE TRIGGER reject_statement_slot BEFORE INSERT ON statement_slots
                 BEGIN SELECT RAISE(ABORT, 'fixture slot insert failed'); END;",
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let session = fixture.wallet.current_session().unwrap();
        let result = fixture
            .wallet
            .allocate_statement_store_allowance(
                &session,
                PRODUCT,
                OnExistingAllowancePolicy::Increase,
            )
            .await;
        let reason = result
            .err()
            .expect("a failed receipt write must fail allocation")
            .to_string();
        assert!(reason.contains("fixture slot insert failed"), "{reason}");
        assert_eq!(
            (
                rows(&fixture.database, "SELECT * FROM allowance_records").await,
                rows(&fixture.database, "SELECT * FROM statement_slots").await,
                submission_count(&fixture.provider),
            ),
            (Vec::<Vec<Value>>::new(), Vec::<Vec<Value>>::new(), 1)
        );
    }));
}
