use super::*;
use crate::test_support::{StubPlatform, test_spawner};
use ed25519_dalek::{Signer, SigningKey};
use futures::executor::block_on;
use sha2::{Digest, Sha256};

const PRODUCT: &str = "receiver.paseo";
fn authority() -> ReceivingAuthority {
    ReceivingAuthority { product_id: PRODUCT.into(), account: "11".repeat(32), environment: "paseo".into(),
        artifact: "22".repeat(32), genesis: "33".repeat(32), generation: 1, os_permission: true, transport_ready: true }
}
fn key() -> SigningKey { SigningKey::from_bytes(&[42; 32]) }
fn watch() -> ReceivingWatch {
    ReceivingWatch { id: "inbox".into(), genesis: authority().genesis, channel: "44".repeat(32),
        topics: vec!["55".repeat(32)], senders: vec![hex::encode(key().verifying_key().as_bytes())],
        expires_at: now() + DAY, muted_until: 0, route: "/inbox".into() }
}
fn setup() -> (Arc<StubPlatform>, ReceivingService) {
    let platform = Arc::new(StubPlatform::default());
    *platform.receiving_authority.lock() = Some(authority());
    platform.receiving_consent.store(true, Ordering::SeqCst);
    let service = ReceivingService::new(platform.clone(), test_spawner());
    (platform, service)
}
// Independent minimal standard Gordian Envelope writer for the real verifier.
fn cbor(out: &mut Vec<u8>, major: u8, value: u64) {
    if value < 24 { out.push(major << 5 | value as u8); }
    else if value <= 255 { out.extend([major << 5 | 24, value as u8]); }
    else if value <= 65535 { out.push(major << 5 | 25); out.extend((value as u16).to_be_bytes()); }
    else { out.push(major << 5 | 26); out.extend((value as u32).to_be_bytes()); }
}
fn leaf(out: &mut Vec<u8>, major: u8, bytes: &[u8]) {
    cbor(out, 6, 201); cbor(out, major, bytes.len() as u64); out.extend(bytes);
}
fn frame(event: u8, created_at: u64, topics: &[String]) -> Vec<u8> {
    let watch = watch();
    let carrier = [1u8, 2, 3];
    let digest = hex::encode(Sha256::digest(carrier));
    let sender = hex::encode(key().verifying_key().as_bytes());
    let event_id = hex::encode([event; 32]);
    let expires_at = created_at + 60_000;
    let signed = serde_json::to_vec(&("truapi:notification:v1", 1, PRODUCT, &watch.genesis,
        &watch.channel, topics, &event_id, created_at, expires_at, &digest, &sender)).unwrap();
    let header = serde_json::json!({ "v":1,"product":PRODUCT,"genesis":watch.genesis,"channel":watch.channel,
        "topics":topics,"eventId":event_id,"createdAt":created_at,"expiresAt":expires_at,
        "ciphertextDigest":digest,"senderKey":sender,"signature":hex::encode(key().sign(&signed).to_bytes()) });
    let header = serde_json::to_vec(&header).unwrap();
    let mut out = Vec::new();
    cbor(&mut out, 6, 200); cbor(&mut out, 4, 2);
    leaf(&mut out, 2, &carrier);
    cbor(&mut out, 5, 1);
    leaf(&mut out, 3, b"truapiNotification"); leaf(&mut out, 3, &header);
    out
}
async fn deliver(service: &ReceivingService, revision: u64, event: u8) -> Result<Vec<ReceivingEvent>, Error> {
    let watch = watch();
    // Actual transport may add topics; signed topics must still cover the watch.
    let topics = vec![watch.topics[0].clone(), "66".repeat(32)];
    service.ingest(PRODUCT, revision, watch.id, watch.genesis, watch.channel,
        topics, frame(event, now(), &watch.topics)).await
}

#[test]
fn missing_backend_is_unsupported_and_os_grant_is_not_consent() {
    block_on(async {
        let service = ReceivingService::new(Arc::new(StubPlatform::default()), test_spawner());
        assert!(!service.status(PRODUCT).await.unwrap().supported);
        assert!(matches!(service.replace(PRODUCT, 0, vec![watch()]).await, Err(Error::Unsupported)));
        let (platform, service) = setup();
        platform.receiving_consent.store(false, Ordering::SeqCst);
        assert!(matches!(service.replace(PRODUCT, 0, vec![watch()]).await, Err(Error::PermissionDenied)));
        assert!(!service.status(PRODUCT).await.unwrap().consent);
    });
}

#[test]
fn registration_and_receipts_survive_all_product_executions_closing() {
    block_on(async {
        let (platform, service) = setup();
        let status = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap();
        let event = deliver(&service, status.revision, 1).await.unwrap().remove(0);
        drop(service);
        let restored = ReceivingService::new(platform, test_spawner());
        assert_eq!(restored.events(PRODUCT, 0).await.unwrap(), vec![event]);
        assert!(deliver(&restored, status.revision, 1).await.unwrap().is_empty());
        assert_eq!(restored.pending().await.unwrap().len(), 1);
    });
}

#[test]
fn foreground_and_read_receipts_win_before_display_reservation() {
    block_on(async {
        let (_, service) = setup();
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let event = deliver(&service, revision, 2).await.unwrap().remove(0);
        service.receipt(PRODUCT, revision, watch().id, event.event_id.clone(), ReceivingReceiptKind::Foreground).await.unwrap();
        assert!(service.prepare_display(PRODUCT, revision, event.event_id.clone()).await.unwrap().is_none());
        service.receipt(PRODUCT, revision, watch().id, event.event_id.clone(), ReceivingReceiptKind::Read).await.unwrap();
        assert!(service.validate_activation(PRODUCT, revision, event.event_id).await.unwrap().is_none());
    });
}

#[test]
fn display_reservation_and_activation_are_distinct_and_replay_safe() {
    block_on(async {
        let (_, service) = setup();
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let event = deliver(&service, revision, 3).await.unwrap().remove(0);
        assert!(service.prepare_display(PRODUCT, revision, event.event_id.clone()).await.unwrap().is_some());
        assert!(service.prepare_display(PRODUCT, revision, event.event_id.clone()).await.unwrap().is_none());
        service.confirm_display(PRODUCT, revision, event.event_id.clone()).await.unwrap();
        let preview = service.validate_activation(PRODUCT, revision, event.event_id.clone()).await.unwrap().unwrap();
        assert_eq!(preview.sequence, 0);
        let activation = service.activate(PRODUCT, revision, event.event_id.clone()).await.unwrap().unwrap();
        assert_eq!(activation.kind, ReceivingEventKind::Activation);
        assert!(service.activate(PRODUCT, revision, event.event_id).await.unwrap().is_none());
        service.acknowledge(PRODUCT, activation.sequence).await.unwrap();
        assert!(service.events(PRODUCT, 0).await.unwrap().iter().all(|e| e.sequence != activation.sequence));
    });
}

#[test]
fn message_receipts_do_not_acknowledge_a_queued_user_activation() {
    block_on(async {
        for kind in [ReceivingReceiptKind::Foreground, ReceivingReceiptKind::Displayed, ReceivingReceiptKind::Read] {
            let (_, service) = setup();
            let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
            let event = deliver(&service, revision, 3).await.unwrap().remove(0);
            service.prepare_display(PRODUCT, revision, event.event_id.clone()).await.unwrap().unwrap();
            service.confirm_display(PRODUCT, revision, event.event_id.clone()).await.unwrap();
            let activation = service.activate(PRODUCT, revision, event.event_id.clone()).await.unwrap().unwrap();
            service.receipt(PRODUCT, revision, watch().id, event.event_id, kind).await.unwrap();
            assert_eq!(service.events(PRODUCT, 0).await.unwrap(), vec![activation.clone()]);
            service.acknowledge(PRODUCT, activation.sequence).await.unwrap();
            assert!(service.events(PRODUCT, 0).await.unwrap().is_empty());
        }
    });
}

#[test]
fn replacement_is_atomic_and_stale_sync_cannot_acknowledge_revoke() {
    block_on(async {
        let (_, service) = setup();
        let first = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap();
        let mut invalid_watch = watch(); invalid_watch.route = "//evil.invalid".into();
        assert!(service.replace(PRODUCT, first.revision, vec![invalid_watch]).await.is_err());
        assert_eq!(service.status(PRODUCT).await.unwrap().revision, first.revision);
        let disabled = service.disable(PRODUCT, first.revision).await.unwrap();
        assert!(!disabled.enabled);
        assert!(!service.synchronized(PRODUCT, first.revision).await.unwrap());
        assert!(service.status(PRODUCT).await.unwrap().sync_pending);
        assert!(matches!(deliver(&service, first.revision, 4).await, Err(Error::Conflict)));
    });
}

#[test]
fn pending_confirmation_cannot_resurrect_revoked_first_registration() {
    block_on(async {
        let (platform, service) = setup();
        let (release, gate) = futures::channel::oneshot::channel();
        *platform.receiving_consent_gate.lock() = Some(gate);
        let replace = service.replace(PRODUCT, 0, vec![watch()]);
        let revoke = async {
            // join polls replace first, which reaches and parks at consent.
            service.revoke(PRODUCT).await.unwrap();
            release.send(()).unwrap();
        };
        let (result, ()) = futures::join!(replace, revoke);
        assert!(matches!(result, Err(Error::Conflict)));
        assert!(!service.status(PRODUCT).await.unwrap().enabled);
    });
}

#[test]
fn failed_durable_write_never_reports_enrollment_or_display_success() {
    block_on(async {
        let (platform, service) = setup();
        platform.receiving_write_failure.store(true, Ordering::SeqCst);
        assert!(matches!(service.replace(PRODUCT, 0, vec![watch()]).await, Err(Error::Storage { .. })));
        assert_eq!(service.status(PRODUCT).await.unwrap().revision, 0);
        platform.receiving_write_failure.store(false, Ordering::SeqCst);
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let event = deliver(&service, revision, 5).await.unwrap().remove(0);
        platform.receiving_write_failure.store(true, Ordering::SeqCst);
        assert!(service.prepare_display(PRODUCT, revision, event.event_id.clone()).await.is_err());
        platform.receiving_write_failure.store(false, Ordering::SeqCst);
        assert!(service.prepare_display(PRODUCT, revision, event.event_id).await.unwrap().is_some());
    });
}

#[test]
fn account_artifact_environment_generation_and_os_changes_revalidate() {
    block_on(async {
        for field in 0..5 {
            let (platform, service) = setup();
            let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
            let mut next = authority();
            match field { 0 => next.account = "77".repeat(32), 1 => next.artifact = "88".repeat(32),
                2 => next.environment = "polkadot".into(), 3 => next.generation += 1, _ => next.os_permission = false }
            *platform.receiving_authority.lock() = Some(next);
            assert!(!service.status(PRODUCT).await.unwrap().enabled);
            assert!(deliver(&service, revision, 6).await.is_err());
            assert!(service.pending().await.unwrap().iter().all(|r| !r.enabled));
        }
    });
}

#[test]
fn sender_channel_source_and_signature_cannot_be_forged() {
    block_on(async {
        let (_, service) = setup();
        let watch = watch();
        let revision = service.replace(PRODUCT, 0, vec![watch.clone()]).await.unwrap().revision;
        let body = frame(7, now(), &watch.topics);
        assert!(service.ingest(PRODUCT, revision, watch.id.clone(), watch.genesis.clone(), "99".repeat(32), watch.topics.clone(), body.clone()).await.is_err());
        assert!(service.ingest(PRODUCT, revision, watch.id.clone(), "99".repeat(32), watch.channel.clone(), watch.topics.clone(), body.clone()).await.is_err());
        let mut altered = body;
        // Subject byte tamper leaves the valid signature over the old digest intact.
        let index = altered.windows(4).position(|w| w == [0x43, 1, 2, 3]).unwrap();
        altered[index + 1] ^= 1;
        assert!(service.ingest(PRODUCT, revision, watch.id, watch.genesis, watch.channel, watch.topics, altered).await.unwrap().is_empty());
    });
}

#[test]
fn capacity_rejection_preserves_prior_state_and_live_replay_entries() {
    block_on(async {
        let (_, service) = setup();
        let first = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap();
        let too_many: Vec<_> = (0..257).map(|i| { let mut w = watch(); w.id = format!("w{i}"); w }).collect();
        assert!(matches!(service.replace(PRODUCT, first.revision, too_many).await, Err(Error::Capacity)));
        for event in 10..14 { assert_eq!(deliver(&service, first.revision, event).await.unwrap().len(), 1); }
        assert!(deliver(&service, first.revision, 14).await.unwrap().is_empty());
        assert!(deliver(&service, first.revision, 10).await.unwrap().is_empty());
        assert_eq!(service.status(PRODUCT).await.unwrap().revision, first.revision);
    });
}

#[test]
fn mute_narrowing_does_not_prompt_and_unmute_does_not_replay_history() {
    block_on(async {
        let (platform, service) = setup();
        let mut watch = watch();
        let first = service.replace(PRODUCT, 0, vec![watch.clone()]).await.unwrap();
        watch.muted_until = u64::MAX;
        let muted = service.replace(PRODUCT, first.revision, vec![watch.clone()]).await.unwrap();
        assert!(deliver(&service, muted.revision, 20).await.unwrap().is_empty());
        assert_eq!(platform.receiving_prompts.load(Ordering::SeqCst), 1);
        watch.muted_until = 0;
        let unmuted = service.replace(PRODUCT, muted.revision, vec![watch.clone()]).await.unwrap();
        assert!(service.ingest(PRODUCT, unmuted.revision, watch.id, watch.genesis, watch.channel,
            watch.topics.clone(), frame(21, now() - 10_000, &watch.topics)).await.unwrap().is_empty());
    });
}

#[test]
fn durable_hourly_budget_is_checked_at_display_not_enrollment() {
    block_on(async {
        let (_, service) = setup();
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let event = deliver(&service, revision, 22).await.unwrap().remove(0);
        let mut ledger = service.load().await.unwrap();
        ledger.records[0].display_attempts = vec![now(); 60];
        service.save(&ledger).await.unwrap();
        assert!(service.prepare_display(PRODUCT, revision, event.event_id).await.unwrap().is_none());
        assert!(service.status(PRODUCT).await.unwrap().enabled);
    });
}

#[test]
fn expired_or_missing_statement_expiry_never_qualifies() {
    use crate::host_logic::statement_store::{StatementField, StatementProof};
    use parity_scale_codec::Encode;
    block_on(async {
        let (_, service) = setup();
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let fields = vec![
            StatementField::Proof(StatementProof::Sr25519 { signature: [0; 64], signer: [0; 32] }),
            StatementField::Expiry(((now() / 1000) - 10) << 32),
            StatementField::Channel([0x44; 32]),
            StatementField::Topic1([0x55; 32]),
            StatementField::Data(frame(30, now(), &watch().topics)),
        ];
        assert!(service.ingest_statement(PRODUCT, revision, watch().id, watch().genesis, fields.encode()).await.unwrap().is_empty());
        let without_expiry: Vec<_> = fields.into_iter().filter(|f| !matches!(f, StatementField::Expiry(_))).collect();
        assert!(service.ingest_statement(PRODUCT, revision, watch().id, watch().genesis, without_expiry.encode()).await.is_err());
    });
}

#[test]
fn independent_authenticated_backfill_candidates_have_separate_receipts() {
    block_on(async {
        let (_, service) = setup();
        let watch = watch();
        let revision = service.replace(PRODUCT, 0, vec![watch.clone()]).await.unwrap().revision;
        let first = frame(31, now(), &watch.topics);
        let second = frame(32, now(), &watch.topics);
        let mut carrier = Vec::new();
        cbor(&mut carrier, 6, 200);
        cbor(&mut carrier, 4, 2);
        carrier.extend(&first[2..]);
        cbor(&mut carrier, 5, 1);
        leaf(&mut carrier, 3, b"history");
        carrier.extend(&second[2..]);
        let accepted = service.ingest(PRODUCT, revision, watch.id.clone(), watch.genesis.clone(), watch.channel.clone(),
            watch.topics.clone(), carrier.clone()).await.unwrap();
        assert_eq!(accepted.len(), 2);
        assert_ne!(accepted[0].event_id, accepted[1].event_id);
        assert!(service.ingest(PRODUCT, revision, watch.id, watch.genesis, watch.channel, watch.topics,
            carrier).await.unwrap().is_empty());
    });
}

#[test]
fn provider_confirmed_display_is_not_a_local_display_authorization() {
    block_on(async {
        let (_, service) = setup();
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let event = deliver(&service, revision, 33).await.unwrap().remove(0);
        service.confirm_display(PRODUCT, revision, event.event_id.clone()).await.unwrap();
        assert!(service.prepare_display(PRODUCT, revision, event.event_id.clone()).await.unwrap().is_none());
        assert!(service.validate_activation(PRODUCT, revision, event.event_id).await.unwrap().is_some());
    });
}

#[test]
fn stale_product_execution_cannot_enroll_under_a_new_verified_artifact() {
    block_on(async {
        let (platform, service) = setup();
        let old_execution = service.for_execution(authority());
        let mut replacement = authority();
        replacement.artifact = "aa".repeat(32);
        *platform.receiving_authority.lock() = Some(replacement.clone());
        assert!(matches!(old_execution.status().await, Err(Error::PermissionDenied)));
        assert!(matches!(old_execution.replace(0, vec![watch()]).await, Err(Error::PermissionDenied)));
        assert_eq!(platform.receiving_prompts.load(Ordering::SeqCst), 0);
        assert!(service.for_execution(replacement).replace(0, vec![watch()]).await.unwrap().enabled);
    });
}

#[test]
fn execution_provenance_is_rechecked_after_receiving_consent() {
    block_on(async {
        let (platform, service) = setup();
        let execution = service.for_execution(authority());
        let (release, gate) = futures::channel::oneshot::channel();
        *platform.receiving_consent_gate.lock() = Some(gate);
        let enroll = execution.replace(0, vec![watch()]);
        let switch = async {
            let mut replacement = authority();
            replacement.generation += 1;
            *platform.receiving_authority.lock() = Some(replacement);
            release.send(()).unwrap();
        };
        let (result, ()) = futures::join!(enroll, switch);
        assert!(matches!(result, Err(Error::PermissionDenied)));
        assert!(!service.status(PRODUCT).await.unwrap().enabled);
    });
}

#[test]
fn explicit_logout_revokes_all_and_does_not_await_transport() {
    block_on(async {
        let (_, service) = setup();
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        service.revoke_all().await.unwrap();
        assert!(!service.status(PRODUCT).await.unwrap().enabled);
        assert!(!service.synchronized(PRODUCT, revision).await.unwrap());
        assert!(service.pending().await.unwrap().iter().all(|r| !r.enabled && r.sync_pending));
    });
}

#[test]
fn foreground_receipt_reports_actual_display_not_a_reserved_claim() {
    block_on(async {
        let (platform, service) = setup();
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let event = deliver(&service, revision, 34).await.unwrap().remove(0);
        service.prepare_display(PRODUCT, revision, event.event_id.clone()).await.unwrap().unwrap();
        let outcome = service.receipt(PRODUCT, revision, watch().id, event.event_id.clone(), ReceivingReceiptKind::Foreground).await.unwrap();
        assert!(!outcome.displayed);
        assert!(outcome.display_pending);
        drop(service);
        let restored = ReceivingService::new(platform, test_spawner());
        let unknown_after_restart = restored.receipt(PRODUCT, revision, watch().id, event.event_id.clone(), ReceivingReceiptKind::Foreground).await.unwrap();
        assert!(!unknown_after_restart.displayed);
        assert!(unknown_after_restart.display_pending);
        restored.confirm_display(PRODUCT, revision, event.event_id.clone()).await.unwrap();
        let confirmed = restored.receipt(PRODUCT, revision, watch().id, event.event_id, ReceivingReceiptKind::Foreground).await.unwrap();
        assert!(confirmed.displayed);
        assert!(!confirmed.display_pending);
    });
}

#[test]
fn known_failed_display_releases_claim_without_claiming_os_success() {
    block_on(async {
        let (_, service) = setup();
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let event = deliver(&service, revision, 35).await.unwrap().remove(0);
        service.prepare_display(PRODUCT, revision, event.event_id.clone()).await.unwrap().unwrap();
        service.receipt(PRODUCT, revision, watch().id, event.event_id.clone(), ReceivingReceiptKind::Foreground).await.unwrap();
        service.cancel_display(PRODUCT, revision, event.event_id.clone()).await.unwrap();
        let fallback = service.receipt(PRODUCT, revision, watch().id, event.event_id.clone(), ReceivingReceiptKind::Foreground).await.unwrap();
        assert!(!fallback.displayed && !fallback.display_pending);
        let shown = service.receipt(PRODUCT, revision, watch().id, event.event_id.clone(), ReceivingReceiptKind::Displayed).await.unwrap();
        assert!(shown.displayed && !shown.display_pending);
        service.cancel_display(PRODUCT, revision, event.event_id.clone()).await.unwrap();
        let repeated = service.receipt(PRODUCT, revision, watch().id, event.event_id, ReceivingReceiptKind::Foreground).await.unwrap();
        assert!(repeated.displayed && !repeated.display_pending);
    });
}

#[test]
fn foreground_before_host_ingest_suppresses_host_without_faking_display() {
    block_on(async {
        let (_, service) = setup();
        let revision = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let outcome = service.receipt(PRODUCT, revision, watch().id, hex::encode([36u8; 32]), ReceivingReceiptKind::Foreground).await.unwrap();
        assert!(!outcome.displayed && !outcome.display_pending);
        assert!(deliver(&service, revision, 36).await.unwrap().is_empty());
    });
}

#[test]
fn token_rotation_preserves_accepted_events_and_fences_old_acknowledgements() {
    block_on(async {
        let (_, service) = setup();
        let old = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap().revision;
        let accepted = deliver(&service, old, 37).await.unwrap().remove(0);
        service.mark_transport_changed(PRODUCT).await.unwrap();
        let current = service.status(PRODUCT).await.unwrap().revision;
        assert!(current > old);
        assert!(!service.synchronized(PRODUCT, old).await.unwrap());
        let events = service.events(PRODUCT, 0).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, accepted.event_id);
        assert_eq!(events[0].revision, current);
        assert!(deliver(&service, current, 37).await.unwrap().is_empty());
        assert!(service.prepare_display(PRODUCT, current, accepted.event_id).await.unwrap().is_some());
    });
}

#[test]
fn new_account_sync_cannot_acknowledge_old_account_revocation() {
    block_on(async {
        let (platform, service) = setup();
        service.replace(PRODUCT, 0, vec![watch()]).await.unwrap();
        let mut second = authority();
        second.account = "bb".repeat(32);
        second.generation += 1;
        *platform.receiving_authority.lock() = Some(second);
        let active = service.replace(PRODUCT, 0, vec![watch()]).await.unwrap();
        let pending = service.pending().await.unwrap();
        let retired = pending.iter().find(|r| !r.enabled).unwrap();
        assert_ne!(retired.revision, active.revision);
        assert!(service.synchronized(PRODUCT, active.revision).await.unwrap());
        let pending = service.pending().await.unwrap();
        assert!(pending.iter().find(|r| !r.enabled).unwrap().sync_pending);
        assert!(!pending.iter().find(|r| r.enabled).unwrap().sync_pending);
        service.revoke_all().await.unwrap();
        let revoked = service.pending().await.unwrap();
        assert_ne!(revoked[0].revision, revoked[1].revision);
    });
}
