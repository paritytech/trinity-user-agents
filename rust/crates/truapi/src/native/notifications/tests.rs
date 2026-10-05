use super::*;
use crate::native::storage::NativeSecrets;
use crate::native::tests::{EventCallbacks, native_host_runtime_config, native_runtime};
use crate::platform::{PermissionAuthorizationRequest, PermissionAuthorizationStatus};
use crate::store::{Db, DbConfig, DbLocation, account_core_db_config, core_migrations};
use futures::executor::block_on;

async fn fixture() -> (Arc<EventCallbacks>, RuntimeStore, NativeNotifications) {
    let callbacks = Arc::new(EventCallbacks::new());
    let database = Db::open(DbConfig {
        location: DbLocation::Memory,
        migrations: core_migrations,
        readers: 1,
    })
    .await
    .unwrap();
    let store = RuntimeStore::open(
        database,
        Arc::new(NativeSecrets(callbacks.clone())),
        [1; 32],
    )
    .await
    .unwrap();
    let storage = Arc::new(NativeStorage::default());
    storage.install(Arc::new(store.clone()));
    let notifications = NativeNotifications::new(storage, callbacks.clone());
    (callbacks, store, notifications)
}

fn future_request() -> HostPushNotificationRequest {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    HostPushNotificationRequest {
        text: "Your turn".to_string(),
        deeplink: Some("chess.dot/game".to_string()),
        scheduled_at: Some(u64::try_from(now).unwrap() + 3_600_000),
    }
}

#[test]
fn failed_os_registration_and_cancellation_are_retried_from_durable_intents() {
    block_on(async {
        let (callbacks, store, notifications) = fixture().await;
        *callbacks.notification_registration_error.lock().unwrap() = Some("OS unavailable".into());
        assert!(
            notifications
                .schedule("chess.dot".into(), future_request())
                .await
                .is_err()
        );
        let mut expected = store.notifications().await.unwrap();
        assert_eq!(expected.len(), 1);
        assert_eq!(expected[0].state, NotificationState::Register);
        assert!(callbacks.pending_notifications.lock().unwrap().is_empty());

        *callbacks.notification_registration_error.lock().unwrap() = None;
        notifications.reconcile().await.unwrap();
        expected[0].state = NotificationState::Registered;
        assert_eq!(store.notifications().await.unwrap(), expected);
        let notification_id = expected[0].notification_id;
        assert!(
            callbacks
                .pending_notifications
                .lock()
                .unwrap()
                .contains_key(&("chess.dot".to_string(), notification_id))
        );

        *callbacks.notification_cancellation_error.lock().unwrap() = Some("OS unavailable".into());
        assert!(
            notifications
                .cancel("chess.dot".into(), notification_id)
                .await
                .is_err()
        );
        expected[0].state = NotificationState::Cancel;
        expected[0].revision += 1;
        assert_eq!(store.notifications().await.unwrap(), expected);
        assert!(store.remove_product("chess.dot".into()).await.is_err());

        *callbacks.notification_cancellation_error.lock().unwrap() = None;
        notifications.reconcile().await.unwrap();
        store.remove_product("chess.dot".into()).await.unwrap();
        assert_eq!(
            (
                store.notifications().await.unwrap(),
                store.products().await.unwrap()
            ),
            (vec![], vec![])
        );
        assert!(callbacks.pending_notifications.lock().unwrap().is_empty());
    });
}

#[test]
fn notification_denial_retains_failed_cancellation_until_product_removal_can_finish() {
    let directory = tempfile::tempdir().unwrap();
    let callbacks = Arc::new(EventCallbacks::new());
    let mut config = native_host_runtime_config();
    config.database_directory = directory.path().to_string_lossy().into_owned();
    let runtime = native_runtime(callbacks.clone(), config).unwrap();
    block_on(async {
        let owner = crate::host_logic::product_account::derive_root_keypair_from_entropy(&[7; 32])
            .unwrap()
            .public
            .to_bytes();
        let database = Db::open(account_core_db_config(directory.path(), &owner))
            .await
            .unwrap();
        let store = RuntimeStore::open(database, Arc::new(NativeSecrets(callbacks.clone())), owner)
            .await
            .unwrap();
        store.ensure_product("chess.dot".into()).await.unwrap();
        let request = future_request();
        store
            .prepare_notification(
                "chess.dot".into(),
                request.text,
                request.deeplink,
                request.scheduled_at.map(i64::try_from).transpose().unwrap(),
                64,
            )
            .await
            .unwrap();
        runtime.reconcile_notifications().await.unwrap();
        *callbacks.notification_cancellation_error.lock().unwrap() = Some("OS unavailable".into());
        let permission = PermissionAuthorizationRequest::Device(
            crate::latest::HostDevicePermissionRequest::Notifications,
        );
        assert!(
            runtime
                .set_permission_authorization_status(
                    "chess.dot".into(),
                    permission.clone(),
                    PermissionAuthorizationStatus::Denied
                )
                .await
                .is_err()
        );
        assert_eq!(
            runtime
                .permission_authorization_status("chess.dot".into(), permission)
                .await
                .unwrap(),
            PermissionAuthorizationStatus::Denied
        );
        assert_eq!(
            runtime.scheduled_notifications().await.unwrap()[0].state,
            NotificationState::Cancel
        );
        assert!(runtime.remove_product("chess.dot".into()).await.is_err());
        assert_eq!(runtime.products().await.unwrap().len(), 1);

        *callbacks.notification_cancellation_error.lock().unwrap() = None;
        runtime.remove_product("chess.dot".into()).await.unwrap();
        assert_eq!(
            (
                runtime.products().await.unwrap(),
                runtime.scheduled_notifications().await.unwrap()
            ),
            (vec![], vec![])
        );
        assert!(callbacks.pending_notifications.lock().unwrap().is_empty());
    });
}

#[test]
fn expired_deliveries_do_not_fill_the_shared_future_queue() {
    block_on(async {
        let (_callbacks, store, notifications) = fixture().await;
        store.ensure_product("chess.dot".into()).await.unwrap();
        for _ in 0..64 {
            let record = store
                .prepare_notification("chess.dot".into(), "expired".into(), None, Some(1), 64)
                .await
                .unwrap();
            store
                .acknowledge_notification(
                    record.notification_id,
                    record.revision,
                    NotificationState::Register,
                )
                .await
                .unwrap();
        }
        let id = notifications
            .schedule("other.dot".into(), future_request())
            .await
            .unwrap();
        let remaining = store.notifications().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].notification_id, id);
    });
}
