//! Host-owned receiving policy. One service is the sole writer for one CoreStorage
//! namespace; browser window/worker adapters must route to the same owner, not
//! independently perform read/modify/write against the slot. No product lifetime
//! owns this service, and no transport request is awaited by product operations.

use std::collections::HashSet;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use crate::latest::{HostNotificationReceiverStatus, HostNotificationReceiptResult, HostNotificationReceivingError as Error,
    ReceivingEvent, ReceivingEventKind, ReceivingReceiptKind, ReceivingWatch};
use crate::platform::{CoreStorageKey, Platform, ProductContext, ReceivingAuthority, ReceivingRegistration};
use crate::subscription::Spawner;
use super::notification_envelope::verify_frames;

const MAX_REGISTRATIONS: usize = 32;
const MAX_WATCHES: usize = 256;
const MAX_SENDERS: usize = 1_000;
const MAX_TOTAL_SENDERS: usize = 10_000;
const MAX_RECEIPTS: usize = 2_048;
const MAX_EVENTS: usize = 128;
const MAX_STATE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const DAY: u64 = 86_400_000;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    version: u8,
    revision: u64,
    sequence: u64,
    records: Vec<Record>,
}
impl Default for Ledger {
    fn default() -> Self { Self { version: 1, revision: 0, sequence: 0, records: Vec::new() } }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    authority: ReceivingAuthority,
    revision: u64,
    consent: bool,
    enabled: bool,
    sync_pending: bool,
    watches: Vec<Watch>,
    receipts: Vec<Receipt>,
    events: Vec<ReceivingEvent>,
    display_attempts: Vec<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Watch {
    policy: ReceivingWatch,
    not_before: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    event_id: String,
    watch_id: String,
    expires_at: u64,
    displayed: bool,
    read: bool,
    activated: bool,
    reserved: bool,
    foreground_seen: bool,
}

/// Minimal trusted backend for a receiver-only browser worker. No wallet,
/// product execution or chain provider is required to validate incoming frames.
#[crate::platform::async_trait]
pub trait ReceivingBackend: Send + Sync {
    /// Resolve the host's current verified scope; absence means unsupported.
    async fn receiver_authority(&self, product: &str) -> Result<Option<ReceivingAuthority>, crate::latest::GenericError>;
    /// Ask trusted UI for durable receiving consent over the disclosed policy.
    async fn receiver_consent(&self, authority: ReceivingAuthority, watches: Vec<ReceivingWatch>) -> Result<bool, crate::latest::GenericError>;
    /// Wake an independent transport synchronizer; never perform network here.
    async fn receiver_changed(&self) -> Result<(), crate::latest::GenericError>;
    /// Read the opaque host-private receiving ledger.
    async fn load(&self) -> Result<Option<Vec<u8>>, crate::latest::GenericError>;
    /// Atomically durably replace that ledger. Single writer is REQUIRED.
    async fn save(&self, bytes: Vec<u8>) -> Result<(), crate::latest::GenericError>;
}

struct PlatformBackend(Arc<dyn Platform>);

#[crate::platform::async_trait]
impl ReceivingBackend for PlatformBackend {
    async fn receiver_authority(&self, product: &str) -> Result<Option<ReceivingAuthority>, crate::latest::GenericError> {
        self.0.receiver_authority(product).await
    }
    async fn receiver_consent(&self, authority: ReceivingAuthority, watches: Vec<ReceivingWatch>) -> Result<bool, crate::latest::GenericError> {
        self.0.receiver_consent(authority, watches).await
    }
    async fn receiver_changed(&self) -> Result<(), crate::latest::GenericError> {
        self.0.receiver_changed().await
    }
    async fn load(&self) -> Result<Option<Vec<u8>>, crate::latest::GenericError> {
        self.0.read_core_storage(CoreStorageKey::NotificationReceiving).await
    }
    async fn save(&self, bytes: Vec<u8>) -> Result<(), crate::latest::GenericError> {
        self.0.write_core_storage(CoreStorageKey::NotificationReceiving, bytes).await
    }
}

#[derive(Default)]
struct WakeState {
    running: AtomicBool,
    requested: AtomicBool,
}

/// Resident host service. Reconstructing it reads the durable ledger on demand.
/// Authority callbacks must read local trusted state, including verified artifact
/// provenance. They must not use a product-supplied identity or network lookup.
pub struct ReceivingService {
    platform: Arc<dyn ReceivingBackend>,
    gate: Mutex<()>,
    spawner: Spawner,
    wake_pending: Arc<WakeState>,
}
impl ReceivingService {
    pub fn new(platform: Arc<dyn Platform>, spawner: Spawner) -> Self {
        Self::from_backend(Arc::new(PlatformBackend(platform)), spawner)
    }

    /// Construct a wallet-free receiver on a minimal trusted storage/authority backend.
    pub fn from_backend(platform: Arc<dyn ReceivingBackend>, spawner: Spawner) -> Self {
        Self { platform, gate: Mutex::new(()), spawner, wake_pending: Arc::new(WakeState::default()) }
    }

    /// Bind a call to immutable, host-verified execution provenance. The snapshot
    /// is supplied only by the trusted connection adapter, never by product wire.
    pub fn for_execution(&self, authority: ReceivingAuthority) -> ReceivingExecution<'_> {
        ReceivingExecution { service: self, authority }
    }

    async fn authority_for(&self, product: &str, expected: Option<&ReceivingAuthority>) -> Result<ReceivingAuthority, Error> {
        let current = self.authority(product).await?;
        if expected.is_some_and(|expected| !same_authority(&current, expected)) {
            return Err(Error::PermissionDenied);
        }
        Ok(current)
    }

    async fn authority(&self, product: &str) -> Result<ReceivingAuthority, Error> {
        ProductContext::new(product.to_owned()).map_err(|_| invalid("invalid product"))?;
        let authority = self.platform.receiver_authority(product).await.map_err(storage)?
            .ok_or(Error::Unsupported)?;
        if authority.product_id != product || !hex32(&authority.account) || !hex32(&authority.artifact)
            || !hex32(&authority.genesis) || authority.environment.is_empty()
            || authority.environment.len() > 128 || authority.generation > MAX_SAFE_INTEGER {
            return Err(Error::PermissionDenied);
        }
        Ok(authority)
    }

    async fn load(&self) -> Result<Ledger, Error> {
        let Some(bytes) = self.platform.load()
            .await.map_err(storage)? else { return Ok(Ledger::default()); };
        if bytes.len() > MAX_STATE_BYTES { return Err(invalid("receiving ledger exceeds budget")); }
        let ledger: Ledger = serde_json::from_slice(&bytes).map_err(|_| invalid("invalid receiving ledger"))?;
        if ledger.version != 1 || ledger.records.len() > MAX_REGISTRATIONS
            || ledger.records.iter().any(|r| r.watches.len() > MAX_WATCHES || r.receipts.len() > MAX_RECEIPTS
                || r.events.len() > MAX_EVENTS || r.display_attempts.len() > 60)
            || ledger.revision > MAX_SAFE_INTEGER || ledger.sequence > MAX_SAFE_INTEGER {
            return Err(invalid("unsupported receiving ledger"));
        }
        Ok(ledger)
    }

    async fn save(&self, ledger: &Ledger) -> Result<(), Error> {
        let bytes = serde_json::to_vec(ledger).map_err(|_| invalid("cannot encode receiving ledger"))?;
        if bytes.len() > MAX_STATE_BYTES { return Err(Error::Capacity); }
        self.platform.save(bytes).await.map_err(storage)
    }

    fn wake(&self) {
        self.wake_pending.requested.store(true, Ordering::Release);
        if self.wake_pending.running.swap(true, Ordering::AcqRel) { return; }
        let platform = self.platform.clone();
        let pending = self.wake_pending.clone();
        (self.spawner)(Box::pin(async move {
            loop {
                pending.requested.store(false, Ordering::Release);
                // A hint, not a transport ACK. Failure leaves durable pending work.
                let _ = platform.receiver_changed().await;
                pending.running.store(false, Ordering::Release);
                // A mutation during the hint must not lose the newer revision.
                if !pending.requested.load(Ordering::Acquire)
                    || pending.running.swap(true, Ordering::AcqRel) { break; }
            }
        }));
    }

    pub async fn status(&self, product: &str) -> Result<HostNotificationReceiverStatus, Error> {
        self.status_scoped(product, None).await
    }

    async fn status_scoped(&self, product: &str, expected: Option<&ReceivingAuthority>) -> Result<HostNotificationReceiverStatus, Error> {
        let _guard = self.gate.lock().await;
        let authority = match self.authority_for(product, expected).await {
            Ok(value) => value,
            Err(Error::Unsupported) => return Ok(HostNotificationReceiverStatus {
                supported: false, os_permission: false, consent: false, enabled: false,
                revision: 0, sync_pending: false, transport_ready: false,
            }),
            Err(error) => return Err(error),
        };
        let ledger = self.load().await?;
        Ok(status(&authority, find(&ledger, &authority)))
    }

    /// Local CAS commit; explicit durable receiving consent is not an OS grant.
    pub async fn replace(&self, product: &str, expected_revision: u64, watches: Vec<ReceivingWatch>)
        -> Result<HostNotificationReceiverStatus, Error> {
        self.replace_scoped(product, expected_revision, watches, None).await
    }

    async fn replace_scoped(&self, product: &str, expected_revision: u64, watches: Vec<ReceivingWatch>,
        expected: Option<&ReceivingAuthority>) -> Result<HostNotificationReceiverStatus, Error> {
        let authority = self.authority_for(product, expected).await?;
        if !authority.os_permission { return Err(Error::PermissionDenied); }
        validate_watches(&watches, &authority, now())?;
        let (fence, needs_consent) = {
            let _guard = self.gate.lock().await;
            let ledger = self.load().await?;
            let record = find(&ledger, &authority);
            check_revision(record, expected_revision)?;
            (ledger.revision, !record.is_some_and(|r| current(r, &authority) && r.consent
                && watches.iter().all(|w| r.watches.iter().any(|old| within_consent(w, &old.policy)))))
        };
        if needs_consent && !self.platform.receiver_consent(authority.clone(), watches.clone())
            .await.map_err(storage)? { return Err(Error::PermissionDenied); }
        let _guard = self.gate.lock().await;
        let fresh = self.authority_for(product, expected).await?;
        if !same_authority(&fresh, &authority) || !fresh.os_permission { return Err(Error::PermissionDenied); }
        let mut ledger = self.load().await?;
        // Revocation, account replacement or a concurrent first enrollment during
        // a prompt fences the result, including when no record existed yet.
        if ledger.revision != fence { return Err(Error::Conflict); }
        check_revision(find(&ledger, &authority), expected_revision)?;
        let timestamp = now();
        validate_watches(&watches, &authority, timestamp)?;
        let position = ledger.records.iter().position(|r| same_scope(&r.authority, &authority));
        if position.is_none() && ledger.records.len() >= MAX_REGISTRATIONS { return Err(Error::Capacity); }
        // A product's previous account/artifact cannot keep receiving accidentally.
        for index in 0..ledger.records.len() {
            if ledger.records[index].authority.product_id == product
                && !same_scope(&ledger.records[index].authority, &authority) {
                let retired_revision = next_revision(&mut ledger)?;
                disable_record(&mut ledger.records[index], retired_revision);
            }
        }
        // Each outbox entry has its own ACK token, including retired scopes.
        let revision = next_revision(&mut ledger)?;
        let position = position.unwrap_or_else(|| {
            ledger.records.push(Record { authority: authority.clone(), revision: 0, consent: false,
                enabled: false, sync_pending: false, watches: Vec::new(), receipts: Vec::new(), events: Vec::new(),
                display_attempts: Vec::new() });
            ledger.records.len() - 1
        });
        let record = &mut ledger.records[position];
        let policies = watches.into_iter().map(|policy| {
            let old = record.watches.iter().find(|w| w.policy.id == policy.id);
            let not_before = old.filter(|w| current(record, &authority)
                && w.policy.genesis == policy.genesis && w.policy.channel == policy.channel && w.policy.topics == policy.topics
                && w.policy.senders == policy.senders && w.policy.muted_until == policy.muted_until)
                .map_or(timestamp, |w| w.not_before);
            Watch { policy, not_before }
        }).collect();
        record.authority = fresh;
        record.watches = policies;
        record.revision = revision;
        record.consent = true;
        record.enabled = !record.watches.is_empty();
        record.sync_pending = true;
        // Old click handles are never reinterpreted under a new policy revision.
        record.events.clear();
        prune(record, timestamp);
        let result = status(&record.authority, Some(record));
        self.save(&ledger).await?;
        self.wake();
        Ok(result)
    }

    pub async fn disable(&self, product: &str, expected_revision: u64)
        -> Result<HostNotificationReceiverStatus, Error> {
        self.disable_scoped(product, expected_revision, None).await
    }

    async fn disable_scoped(&self, product: &str, expected_revision: u64,
        expected: Option<&ReceivingAuthority>) -> Result<HostNotificationReceiverStatus, Error> {
        let _guard = self.gate.lock().await;
        let authority = self.authority_for(product, expected).await?;
        let mut ledger = self.load().await?;
        check_revision(find(&ledger, &authority), expected_revision)?;
        let revision = next_revision(&mut ledger)?;
        for record in &mut ledger.records {
            if same_scope(&record.authority, &authority) { disable_record(record, revision); }
        }
        let result = status(&authority, find(&ledger, &authority));
        self.save(&ledger).await?;
        self.wake();
        Ok(result)
    }

    /// Trusted logout/deletion path, independent of active account availability.
    /// Invoke before deleting account data. A failed durable write is an error.
    pub async fn revoke(&self, product: &str) -> Result<(), Error> {
        let _guard = self.gate.lock().await;
        let mut ledger = self.load().await?;
        next_revision(&mut ledger)?; // Fence even a first enrollment still in consent.
        for index in 0..ledger.records.len() {
            if ledger.records[index].authority.product_id == product {
                let revision = next_revision(&mut ledger)?;
                disable_record(&mut ledger.records[index], revision);
            }
        }
        self.save(&ledger).await?;
        self.wake();
        Ok(())
    }

    /// Explicit host logout revokes locally without contacting any transport.
    /// Ordinary runtime disposal must not call this method.
    pub async fn revoke_all(&self) -> Result<(), Error> {
        let _guard = self.gate.lock().await;
        let mut ledger = self.load().await?;
        next_revision(&mut ledger)?;
        for index in 0..ledger.records.len() {
            let revision = next_revision(&mut ledger)?;
            disable_record(&mut ledger.records[index], revision);
        }
        self.save(&ledger).await?;
        self.wake();
        Ok(())
    }

    /// Provider token rotation invalidates old synchronization results and handles.
    pub async fn mark_transport_changed(&self, product: &str) -> Result<(), Error> {
        let _guard = self.gate.lock().await;
        let mut ledger = self.load().await?;
        next_revision(&mut ledger)?;
        for index in 0..ledger.records.len() {
            if ledger.records[index].authority.product_id == product {
                let revision = next_revision(&mut ledger)?;
                let record = &mut ledger.records[index];
                record.revision = revision;
                record.sync_pending = true;
                // Rotation is not a new watch policy: preserve accepted events
                // and replay receipts, but fence stale transport acknowledgements.
                for event in &mut record.events { event.revision = revision; }
            }
        }
        self.save(&ledger).await?;
        self.wake();
        Ok(())
    }

    /// Local outbox snapshot. Transport must strip routes and use opaque provider
    /// handles. Never send the artifact/account to a provider as notification text.
    pub async fn pending(&self) -> Result<Vec<ReceivingRegistration>, Error> {
        let _guard = self.gate.lock().await;
        let mut ledger = self.load().await?;
        let mut changed = false;
        let mut available = [true; MAX_REGISTRATIONS];
        for index in 0..ledger.records.len() {
            if !ledger.records[index].enabled { continue; }
            let product = &ledger.records[index].authority.product_id;
            let live = self.authority(product).await;
            if live.is_err() {
                available[index] = false;
                continue;
            }
            // A temporarily unavailable/locked authority pauses local handling;
            // only a positively resolved changed scope invalidates the grant.
            if live.as_ref().is_ok_and(|a| !current(&ledger.records[index], a) || !a.os_permission) {
                let revision = next_revision(&mut ledger)?;
                disable_record(&mut ledger.records[index], revision);
                changed = true;
            }
        }
        if changed { self.save(&ledger).await?; }
        Ok(ledger.records.iter().enumerate().filter(|(index, _)| available[*index]).map(|(_, r)| ReceivingRegistration {
            authority: r.authority.clone(), revision: r.revision, enabled: r.enabled,
            watches: r.watches.iter().map(|w| w.policy.clone()).collect(), sync_pending: r.sync_pending,
        }).collect())
    }

    pub async fn synchronized(&self, product: &str, revision: u64) -> Result<bool, Error> {
        let _guard = self.gate.lock().await;
        let mut ledger = self.load().await?;
        let mut matched = false;
        for record in &mut ledger.records {
            if record.authority.product_id == product && record.revision == revision && record.sync_pending {
                record.sync_pending = false;
                matched = true;
            }
        }
        if matched { self.save(&ledger).await?; }
        Ok(matched)
    }

    pub async fn receipt(&self, product: &str, revision: u64, watch_id: String, event_id: String,
        kind: ReceivingReceiptKind) -> Result<HostNotificationReceiptResult, Error> {
        self.receipt_scoped(product, revision, watch_id, event_id, kind, None).await
    }

    async fn receipt_scoped(&self, product: &str, revision: u64, watch_id: String, event_id: String,
        kind: ReceivingReceiptKind, expected: Option<&ReceivingAuthority>) -> Result<HostNotificationReceiptResult, Error> {
        if !hex32(&event_id) { return Err(invalid("invalid receipt event id")); }
        let _guard = self.gate.lock().await;
        let authority = self.authority_for(product, expected).await?;
        let mut ledger = self.load().await?;
        let record = find_mut(&mut ledger, &authority).ok_or(Error::Conflict)?;
        require_record(record, &authority, revision)?;
        let timestamp = now();
        if !record.watches.iter().any(|w| w.policy.id == watch_id && w.policy.expires_at > timestamp) {
            return Err(invalid("unknown receipt watch"));
        }
        prune(record, timestamp);
        let receipt = if let Some(index) = record.receipts.iter().position(|r| r.event_id == event_id) {
            if record.receipts[index].watch_id != watch_id { return Err(invalid("receipt watch mismatch")); }
            &mut record.receipts[index]
        } else {
            if record.receipts.len() >= MAX_RECEIPTS { return Err(Error::Capacity); }
            record.receipts.push(Receipt { event_id: event_id.clone(), watch_id, expires_at: timestamp + DAY,
                displayed: false, read: false, activated: false, reserved: false, foreground_seen: false });
            record.receipts.last_mut().expect("just inserted")
        };
        match kind {
            ReceivingReceiptKind::Foreground => receipt.foreground_seen = true,
            ReceivingReceiptKind::Read => receipt.read = true,
            ReceivingReceiptKind::Displayed => {
                receipt.displayed = true;
                receipt.reserved = false;
            }
        }
        let result = HostNotificationReceiptResult {
            displayed: receipt.displayed,
            display_pending: receipt.reserved && !receipt.displayed,
        };
        // Message catch-up can race the click that opened the product. Only an
        // explicit event acknowledgement consumes that user's queued navigation.
        record.events.retain(|event| event.event_id != event_id || event.kind == ReceivingEventKind::Activation);
        self.save(&ledger).await?;
        Ok(result)
    }

    /// Release a local display reservation only when the platform knows its
    /// presentation failed. Unknown/crashed outcomes stay pending until expiry:
    /// neither a restart nor a timer is evidence that no OS alert was shown.
    pub async fn cancel_display(&self, product: &str, revision: u64, event_id: String) -> Result<(), Error> {
        let _guard = self.gate.lock().await;
        let authority = self.authority(product).await?;
        let mut ledger = self.load().await?;
        let record = find_mut(&mut ledger, &authority).ok_or(Error::Conflict)?;
        require_record(record, &authority, revision)?;
        let receipt = record.receipts.iter_mut().find(|r| r.event_id == event_id && r.expires_at > now())
            .ok_or_else(|| invalid("unknown display handle"))?;
        if receipt.displayed { return Ok(()); }
        receipt.reserved = false;
        self.save(&ledger).await
    }

    /// Final local display gate after the host's foreground grace period. A
    /// durable reservation prevents two callbacks from authorizing the same OS
    /// alert. A crash after reservation may lose an alert, never duplicate it.
    pub async fn prepare_display(&self, product: &str, revision: u64, event_id: String)
        -> Result<Option<ReceivingEvent>, Error> {
        let _guard = self.gate.lock().await;
        let authority = self.authority(product).await?;
        let mut ledger = self.load().await?;
        let record = find_mut(&mut ledger, &authority).ok_or(Error::Conflict)?;
        require_record(record, &authority, revision)?;
        if !authority.os_permission { return Err(Error::PermissionDenied); }
        let timestamp = now();
        prune(record, timestamp);
        let Some(event) = record.events.iter().find(|e| e.event_id == event_id
            && e.revision == revision && e.kind == ReceivingEventKind::Delivery).cloned()
            else { return Ok(None); };
        let Some(receipt) = record.receipts.iter_mut().find(|r| r.event_id == event_id)
            else { return Ok(None); };
        if receipt.read || receipt.foreground_seen || receipt.displayed || receipt.reserved { return Ok(None); }
        if !record.watches.iter().any(|w| w.policy.id == event.watch_id
            && w.policy.expires_at > timestamp && w.policy.muted_until <= timestamp) { return Ok(None); }
        if record.display_attempts.len() >= 60 { return Ok(None); }
        receipt.reserved = true;
        record.display_attempts.push(timestamp);
        self.save(&ledger).await?;
        Ok(Some(event))
    }

    /// Decode the canonical Statement Store shape, not an application codec.
    /// The source genesis is supplied by the host's selected chain connection.
    pub async fn ingest_statement(&self, product: &str, revision: u64, watch_id: String,
        actual_genesis: String, statement: Vec<u8>) -> Result<Vec<ReceivingEvent>, Error> {
        if statement.len() > 256 * 1024 { return Err(Error::Capacity); }
        let statement = crate::host_logic::statement_store::decode_signed_statement(&statement)
            .map_err(|_| invalid("invalid Statement Store statement"))?;
        let expiry = statement.expiry.ok_or_else(|| invalid("statement has no expiry"))?;
        if crate::host_logic::statement_store::statement_expiry_elapsed(expiry, now() / 1000) {
            return Ok(Vec::new());
        }
        let topics = statement.topics.iter().map(hex::encode).collect();
        let channel = hex::encode(statement.channel.ok_or_else(|| invalid("statement has no channel"))?);
        let frame = statement.data.ok_or_else(|| invalid("statement has no frame"))?;
        self.ingest(product, revision, watch_id, actual_genesis, channel, topics, frame).await
    }

    /// Called only after the host actually displays an accepted delivery.
    pub async fn confirm_display(&self, product: &str, revision: u64, event_id: String) -> Result<(), Error> {
        let _guard = self.gate.lock().await;
        let authority = self.authority(product).await?;
        let mut ledger = self.load().await?;
        let record = find_mut(&mut ledger, &authority).ok_or(Error::Conflict)?;
        require_record(record, &authority, revision)?;
        let receipt = record.receipts.iter_mut().find(|r| r.event_id == event_id && r.expires_at > now())
            .ok_or_else(|| invalid("unknown display handle"))?;
        // APNs may already have rendered an advisory alert before our callback.
        // This is a trusted report of actual display, never display permission.
        if !receipt.reserved && !receipt.displayed && record.display_attempts.len() < 60 {
            record.display_attempts.push(now());
        }
        receipt.displayed = true;
        receipt.reserved = false;
        self.save(&ledger).await
    }

    /// Validate actual source and the complete authenticated frame before minting
    /// a local handle. Provider payload IDs alone must never call this path.
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep observed source fields explicit and preserve the host receiving API"
    )]
    pub async fn ingest(&self, product: &str, revision: u64, watch_id: String,
        actual_genesis: String, actual_channel: String, actual_topics: Vec<String>, frame: Vec<u8>)
        -> Result<Vec<ReceivingEvent>, Error> {
        let _guard = self.gate.lock().await;
        let authority = self.authority(product).await?;
        let mut ledger = self.load().await?;
        let record = find_mut(&mut ledger, &authority).ok_or(Error::Conflict)?;
        require_record(record, &authority, revision)?;
        if !authority.os_permission { return Err(Error::PermissionDenied); }
        let timestamp = now();
        prune(record, timestamp);
        let watch = record.watches.iter().find(|w| w.policy.id == watch_id)
            .ok_or_else(|| invalid("unknown delivery watch"))?;
        if watch.policy.expires_at <= timestamp || watch.policy.muted_until > timestamp { return Ok(Vec::new()); }
        if watch.policy.genesis != actual_genesis || watch.policy.channel != actual_channel
            || !watch.policy.topics.iter().all(|topic| actual_topics.contains(topic)) {
            return Err(invalid("delivery source mismatch"));
        }
        let verified = verify_frames(&frame, &actual_genesis, &actual_channel, &actual_topics, timestamp)
            .map_err(|reason| Error::InvalidRequest { reason })?;
        let mut accepted = Vec::new();
        for candidate in verified {
            let record = find_mut(&mut ledger, &authority).expect("record retained");
            let watch = record.watches.iter().find(|w| w.policy.id == watch_id).expect("watch retained");
            let header = candidate.header;
            if header.product != product || !watch.policy.senders.contains(&header.sender_key)
                || !watch.policy.topics.iter().all(|topic| header.topics.contains(topic))
                || header.created_at < watch.not_before || header.created_at < watch.policy.muted_until
                || record.receipts.iter().any(|r| r.event_id == header.event_id) { continue; }
            if record.receipts.len() >= MAX_RECEIPTS || record.events.len() >= MAX_EVENTS
                || record.receipts.iter().filter(|r| !r.displayed && !r.foreground_seen && !r.read && !r.reserved).count() >= 4 {
                // Never evict a live replay entry to admit new traffic. Commit
                // the bounded accepted prefix, leaving excess candidates fresh.
                break;
            }
            let route = watch.policy.route.clone();
            let expires_at = header.expires_at.min(watch.policy.expires_at);
            record.receipts.push(Receipt { event_id: header.event_id.clone(), watch_id: watch_id.clone(),
                expires_at: header.expires_at, displayed: false, read: false, activated: false, reserved: false, foreground_seen: false });
            let sequence = next_sequence(&mut ledger)?;
            let event = ReceivingEvent { sequence, revision, watch_id: watch_id.clone(), event_id: header.event_id,
                kind: ReceivingEventKind::Delivery, route, expires_at };
            find_mut(&mut ledger, &authority).expect("record retained").events.push(event.clone());
            accepted.push(event);
        }
        if !accepted.is_empty() {
            let fresh = self.authority(product).await?;
            if !same_authority(&fresh, &authority) || !fresh.os_permission { return Err(Error::PermissionDenied); }
            self.save(&ledger).await?;
        }
        Ok(accepted)
    }

    /// Preview a current click handle before opening the verified product.
    /// Sequence zero is a preview, not an event; call activate after readiness.
    pub async fn validate_activation(&self, product: &str, revision: u64, event_id: String)
        -> Result<Option<ReceivingEvent>, Error> {
        let _guard = self.gate.lock().await;
        let authority = self.authority(product).await?;
        let ledger = self.load().await?;
        let record = find(&ledger, &authority).ok_or(Error::Conflict)?;
        require_record(record, &authority, revision)?;
        let timestamp = now();
        let Some(receipt) = record.receipts.iter().find(|r| r.event_id == event_id
            && r.expires_at > timestamp && !r.read && !r.activated) else { return Ok(None); };
        let Some(watch) = record.watches.iter().find(|w| w.policy.id == receipt.watch_id
            && w.policy.expires_at > timestamp && w.policy.muted_until <= timestamp) else { return Ok(None); };
        Ok(Some(ReceivingEvent { sequence: 0, revision, watch_id: receipt.watch_id.clone(),
            event_id, kind: ReceivingEventKind::Activation, route: watch.policy.route.clone(),
            expires_at: receipt.expires_at.min(watch.policy.expires_at) }))
    }

    /// User activation resolves only a durable accepted local handle. The shell
    /// may focus/open its verified product; it must never auto-switch accounts.
    pub async fn activate(&self, product: &str, revision: u64, event_id: String)
        -> Result<Option<ReceivingEvent>, Error> {
        let _guard = self.gate.lock().await;
        let authority = self.authority(product).await?;
        let mut ledger = self.load().await?;
        let record = find_mut(&mut ledger, &authority).ok_or(Error::Conflict)?;
        require_record(record, &authority, revision)?;
        let timestamp = now();
        prune(record, timestamp);
        let receipt = record.receipts.iter_mut().find(|r| r.event_id == event_id)
            .ok_or_else(|| invalid("unknown activation handle"))?;
        if receipt.read || receipt.activated { return Ok(None); }
        let watch = record.watches.iter().find(|w| w.policy.id == receipt.watch_id
            && w.policy.expires_at > timestamp && w.policy.muted_until <= timestamp)
            .ok_or(Error::PermissionDenied)?;
        if record.events.len() >= MAX_EVENTS { return Err(Error::Capacity); }
        let watch_id = receipt.watch_id.clone();
        let route = watch.policy.route.clone();
        let expires_at = receipt.expires_at.min(watch.policy.expires_at);
        receipt.activated = true;
        let sequence = next_sequence(&mut ledger)?;
        let event = ReceivingEvent { sequence, revision, watch_id, event_id,
            kind: ReceivingEventKind::Activation, route, expires_at };
        find_mut(&mut ledger, &authority).expect("record retained").events.push(event.clone());
        self.save(&ledger).await?;
        Ok(Some(event))
    }

    pub async fn events(&self, product: &str, after_sequence: u64) -> Result<Vec<ReceivingEvent>, Error> {
        self.events_scoped(product, after_sequence, None).await
    }

    async fn events_scoped(&self, product: &str, after_sequence: u64, expected: Option<&ReceivingAuthority>) -> Result<Vec<ReceivingEvent>, Error> {
        let _guard = self.gate.lock().await;
        let authority = self.authority_for(product, expected).await?;
        let ledger = self.load().await?;
        let Some(record) = find(&ledger, &authority) else { return Ok(Vec::new()); };
        if !current(record, &authority) || !record.enabled || !record.consent { return Ok(Vec::new()); }
        let timestamp = now();
        Ok(record.events.iter().filter(|e| e.sequence > after_sequence && e.expires_at > timestamp
            && e.revision == record.revision).cloned().collect())
    }

    /// Acknowledge exactly one event; unrelated delivery/activation stays queued.
    pub async fn acknowledge(&self, product: &str, sequence: u64) -> Result<(), Error> {
        self.acknowledge_scoped(product, sequence, None).await
    }

    async fn acknowledge_scoped(&self, product: &str, sequence: u64, expected: Option<&ReceivingAuthority>) -> Result<(), Error> {
        let _guard = self.gate.lock().await;
        let authority = self.authority_for(product, expected).await?;
        let mut ledger = self.load().await?;
        let record = find_mut(&mut ledger, &authority).ok_or(Error::Conflict)?;
        if !current(record, &authority) { return Err(Error::PermissionDenied); }
        record.events.retain(|event| event.sequence != sequence);
        self.save(&ledger).await
    }
}

fn now() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(target_arch = "wasm32")]
    use web_time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}
fn invalid(reason: &str) -> Error { Error::InvalidRequest { reason: reason.to_owned() } }
fn storage(error: crate::latest::GenericError) -> Error { Error::Storage { reason: error.reason } }
fn hex32(value: &str) -> bool { value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn same_scope(a: &ReceivingAuthority, b: &ReceivingAuthority) -> bool {
    a.product_id == b.product_id && a.account == b.account && a.environment == b.environment
        && a.artifact == b.artifact && a.genesis == b.genesis
}
fn same_authority(a: &ReceivingAuthority, b: &ReceivingAuthority) -> bool { same_scope(a, b) && a.generation == b.generation }
fn current(record: &Record, authority: &ReceivingAuthority) -> bool { same_authority(&record.authority, authority) }
fn find<'a>(ledger: &'a Ledger, authority: &ReceivingAuthority) -> Option<&'a Record> {
    ledger.records.iter().find(|r| same_scope(&r.authority, authority))
}
fn find_mut<'a>(ledger: &'a mut Ledger, authority: &ReceivingAuthority) -> Option<&'a mut Record> {
    ledger.records.iter_mut().find(|r| same_scope(&r.authority, authority))
}
fn check_revision(record: Option<&Record>, expected: u64) -> Result<(), Error> {
    if record.map_or(0, |r| r.revision) != expected { Err(Error::Conflict) } else { Ok(()) }
}
fn next_revision(ledger: &mut Ledger) -> Result<u64, Error> {
    ledger.revision = ledger.revision.checked_add(1).filter(|n| *n <= MAX_SAFE_INTEGER).ok_or(Error::Capacity)?;
    Ok(ledger.revision)
}
fn next_sequence(ledger: &mut Ledger) -> Result<u64, Error> {
    ledger.sequence = ledger.sequence.checked_add(1).filter(|n| *n <= MAX_SAFE_INTEGER).ok_or(Error::Capacity)?;
    Ok(ledger.sequence)
}
fn disable_record(record: &mut Record, revision: u64) {
    record.revision = revision;
    record.enabled = false;
    record.consent = false;
    record.sync_pending = true;
    record.watches.clear();
    record.events.clear();
}
fn require_record(record: &Record, authority: &ReceivingAuthority, revision: u64) -> Result<(), Error> {
    if record.revision != revision { return Err(Error::Conflict); }
    if !current(record, authority) || !record.consent || !record.enabled { return Err(Error::PermissionDenied); }
    Ok(())
}
fn prune(record: &mut Record, timestamp: u64) {
    record.receipts.retain(|r| r.expires_at > timestamp);
    record.events.retain(|e| e.expires_at > timestamp);
    record.display_attempts.retain(|at| at.saturating_add(3_600_000) > timestamp);
}
fn status(authority: &ReceivingAuthority, record: Option<&Record>) -> HostNotificationReceiverStatus {
    let valid = record.is_some_and(|r| current(r, authority));
    HostNotificationReceiverStatus { supported: true, os_permission: authority.os_permission,
        transport_ready: authority.transport_ready, consent: valid && record.is_some_and(|r| r.consent),
        enabled: valid && authority.os_permission && record.is_some_and(|r| r.enabled),
        revision: record.map_or(0, |r| r.revision), sync_pending: record.is_some_and(|r| r.sync_pending) }
}
fn within_consent(new: &ReceivingWatch, old: &ReceivingWatch) -> bool {
    new.genesis == old.genesis && new.channel == old.channel && new.expires_at <= old.expires_at
        && old.topics.iter().all(|topic| new.topics.contains(topic))
        && new.senders.iter().all(|key| old.senders.contains(key))
}
fn validate_watches(watches: &[ReceivingWatch], authority: &ReceivingAuthority, timestamp: u64) -> Result<(), Error> {
    if watches.len() > MAX_WATCHES { return Err(Error::Capacity); }
    let mut ids = HashSet::with_capacity(watches.len());
    let mut senders = 0usize;
    for watch in watches {
        if watch.id.is_empty() || watch.id.len() > 128 || !watch.id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
            || !ids.insert(&watch.id) { return Err(invalid("invalid or duplicate watch id")); }
        if watch.genesis != authority.genesis || !hex32(&watch.genesis) { return Err(Error::PermissionDenied); }
        if !hex32(&watch.channel) { return Err(invalid("invalid channel")); }
        if watch.topics.is_empty() || watch.topics.len() > 4 || watch.topics.iter().any(|t| !hex32(t))
            || watch.topics.iter().collect::<HashSet<_>>().len() != watch.topics.len() { return Err(invalid("invalid topics")); }
        senders += watch.senders.len();
        if watch.senders.is_empty() || watch.senders.len() > MAX_SENDERS || senders > MAX_TOTAL_SENDERS { return Err(Error::Capacity); }
        if watch.senders.iter().any(|s| !hex32(s)) || watch.senders.iter().collect::<HashSet<_>>().len() != watch.senders.len() {
            return Err(invalid("invalid sender policy"));
        }
        if watch.expires_at <= timestamp || watch.expires_at > timestamp.saturating_add(30 * DAY)
            || watch.expires_at > MAX_SAFE_INTEGER || (watch.muted_until > MAX_SAFE_INTEGER && watch.muted_until != u64::MAX) {
            return Err(invalid("invalid watch timestamps"));
        }
        // Conservative relative routes: reject encoding tricks, separators and
        // traversal rather than interpreting them differently in native/browser.
        if watch.route.len() > 512 || !watch.route.starts_with('/') || watch.route.starts_with("//")
            || watch.route.bytes().any(|b| !b.is_ascii() || b.is_ascii_control() || b"\\%?#:".contains(&b))
            || watch.route.split('/').any(|part| part == "." || part == "..") {
            return Err(invalid("invalid product-relative route"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// Product-call view tied to the verified execution rather than a mutable global
/// product-name lookup. Background callbacks use the resident service directly.
pub struct ReceivingExecution<'a> {
    service: &'a ReceivingService,
    authority: ReceivingAuthority,
}

impl ReceivingExecution<'_> {
    /// Read receiver state without accepting a stale execution's authority.
    pub async fn status(&self) -> Result<HostNotificationReceiverStatus, Error> {
        self.service.status_scoped(&self.authority.product_id, Some(&self.authority)).await
    }
    /// Atomically replace policy, rechecking immutable provenance after consent.
    pub async fn replace(&self, expected_revision: u64, watches: Vec<ReceivingWatch>) -> Result<HostNotificationReceiverStatus, Error> {
        self.service.replace_scoped(&self.authority.product_id, expected_revision, watches, Some(&self.authority)).await
    }
    /// Disable only the calling execution's current account scope.
    pub async fn disable(&self, expected_revision: u64) -> Result<HostNotificationReceiverStatus, Error> {
        self.service.disable_scoped(&self.authority.product_id, expected_revision, Some(&self.authority)).await
    }
    /// Record product-confirmed foreground/read evidence.
    pub async fn receipt(&self, revision: u64, watch_id: String, event_id: String, kind: ReceivingReceiptKind) -> Result<HostNotificationReceiptResult, Error> {
        self.service.receipt_scoped(&self.authority.product_id, revision, watch_id, event_id, kind, Some(&self.authority)).await
    }
    /// Read durable events belonging to the verified execution.
    pub async fn events(&self, after_sequence: u64) -> Result<Vec<ReceivingEvent>, Error> {
        self.service.events_scoped(&self.authority.product_id, after_sequence, Some(&self.authority)).await
    }
    /// Acknowledge exactly one event under the same authority fence.
    pub async fn acknowledge(&self, sequence: u64) -> Result<(), Error> {
        self.service.acknowledge_scoped(&self.authority.product_id, sequence, Some(&self.authority)).await
    }
}
