use std::collections::HashMap;

use futures::StreamExt;
use futures::stream::BoxStream;
use parity_scale_codec::{Decode, Encode};
use rusqlite::{OptionalExtension, Row, params};

use super::RuntimeStore;
use crate::platform::{
    CoreStorageKey, PermissionAuthorizationRequest, PermissionAuthorizationStatus,
    normalize_product_identifier,
};
use crate::store::DbError;

/// Authenticated peer identity and durable lifecycle progress.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PairedHostRecord {
    /// Authenticated statement account identifying the peer.
    pub peer_statement: [u8; 32],
    /// Authenticated encryption identity identifying the peer.
    pub peer_encryption: [u8; 32],
    /// Pairing lifecycle status owned by the SSO service.
    pub status: String,
    /// First registration time in Unix milliseconds.
    pub added_at: i64,
    /// Latest lifecycle change in Unix milliseconds.
    pub updated_at: i64,
    /// Pending outbound device update time in Unix milliseconds.
    pub outgoing_update_at: Option<i64>,
    /// Latest offered synchronization identifier.
    pub last_sync_offer_id: Option<String>,
    /// Display metadata, excluded from authorization.
    pub metadata: HashMap<String, String>,
}

/// Public allowance history, without private signing material.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AllowanceRecord {
    /// Chain whose resource account was allocated.
    pub chain: [u8; 32],
    /// Resource identifier supplied by the allocation owner.
    pub resource: String,
    /// Account receiving the allocation.
    pub account: [u8; 32],
    /// Initial allocation time in Unix milliseconds.
    pub allocated_at: i64,
    /// Priority for resources without detailed statement slots.
    pub priority: Option<i64>,
    /// Renewal period for resources without detailed statement slots.
    pub last_renewed_period: Option<i64>,
}

/// One authoritative statement-store slot assignment.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StatementSlotRecord {
    /// Chain carrying this collection.
    pub chain: [u8; 32],
    /// Collection whose slot is assigned.
    pub collection: String,
    /// Allocation period.
    pub period: i64,
    /// Slot within the period.
    pub slot: i64,
    /// Account assigned to the slot.
    pub account: [u8; 32],
    /// Renewal priority of this assignment.
    pub priority: i64,
    /// Latest allocation or renewal time in Unix milliseconds.
    pub last_allocated_or_renewed_at: i64,
}

/// Persisted permission decision for native administration.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PermissionRecord {
    /// Product whose permission was decided.
    pub product_id: String,
    /// Canonical permission scope.
    pub request: PermissionAuthorizationRequest,
    /// Durable authorization without any transient OS permission projection.
    pub status: PermissionAuthorizationStatus,
}

/// Display catalog, never an authority for product permissions.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProductRecord {
    /// Normalized product identifier.
    pub product_id: String,
    /// Display name.
    pub name: String,
    /// Content-addressed icon reference.
    pub icon_cid: Option<String>,
    /// Icon media format.
    pub icon_format: Option<String>,
    /// Optional worker development URL override.
    pub worker_url_override: Option<String>,
}

/// Durable OS work remaining for a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NotificationState {
    /// Register with the OS before reporting scheduling success.
    Register,
    /// The OS acknowledged registration.
    Registered,
    /// Cancel with the OS before deleting the row.
    Cancel,
}

/// Notification payload retained until OS reconciliation completes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledNotificationRecord {
    /// Product that owns the notification.
    pub product_id: String,
    /// Durable OS request identifier within the account.
    pub notification_id: u32,
    /// Display payload.
    pub text: String,
    /// Product deeplink opened by the notification.
    pub deeplink: Option<String>,
    /// Unix milliseconds, absent for immediate delivery.
    pub scheduled_at: Option<i64>,
    /// Pending OS reconciliation action.
    pub state: NotificationState,
    /// Revision fencing late OS acknowledgments.
    pub revision: u64,
}

fn product_id(value: &str) -> Result<String, DbError> {
    normalize_product_identifier(value).map_err(|error| DbError::InvalidRecord(error.to_string()))
}

fn product_row(row: &Row<'_>) -> rusqlite::Result<ProductRecord> {
    Ok(ProductRecord {
        product_id: row.get(0)?,
        name: row.get(1)?,
        icon_cid: row.get(2)?,
        icon_format: row.get(3)?,
        worker_url_override: row.get(4)?,
    })
}

fn notification_row(row: &Row<'_>) -> rusqlite::Result<ScheduledNotificationRecord> {
    let state: String = row.get(6)?;
    Ok(ScheduledNotificationRecord {
        product_id: row.get(0)?,
        notification_id: row.get(1)?,
        text: row.get(2)?,
        deeplink: row.get(3)?,
        scheduled_at: row.get(4)?,
        revision: {
            let revision: i64 = row.get(5)?;
            u64::try_from(revision)
                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(5, revision))?
        },
        state: match state.as_str() {
            "register" => NotificationState::Register,
            "registered" => NotificationState::Registered,
            "cancel" => NotificationState::Cancel,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
    })
}

fn next_id(transaction: &rusqlite::Transaction<'_>, name: &str) -> Result<u32, DbError> {
    let previous: Option<u32> = transaction
        .query_row(
            "SELECT value FROM runtime_sequences WHERE name=?1",
            [name],
            |row| row.get(0),
        )
        .optional()?;
    let value = previous
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| DbError::InvalidRecord("identifier space exhausted".into()))?;
    transaction.execute("INSERT INTO runtime_sequences VALUES (?1,?2) ON CONFLICT(name) DO UPDATE SET value=excluded.value", params![name, value])?;
    Ok(value)
}

impl RuntimeStore {
    /// Persist peer lifecycle and replace metadata in one transaction.
    pub async fn save_paired_host(&self, peer: PairedHostRecord) -> Result<(), DbError> {
        let wallet = self.owner;
        self.write_records(move |transaction| {
            transaction.execute(
                "
                INSERT INTO paired_hosts VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON
                CONFLICT(wallet,peer_statement,peer_encryption) DO UPDATE SET status=excluded.status,
                updated_at=excluded.updated_at,
                outgoing_update_at=excluded.outgoing_update_at,last_sync_offer_id=excluded.last_sync_offer_id
            ",
                params![wallet, peer.peer_statement, peer.peer_encryption, peer.status, peer.added_at, peer.updated_at, peer.outgoing_update_at, peer.last_sync_offer_id],
            )?;
            transaction.execute("DELETE FROM paired_host_metadata WHERE wallet=?1 AND peer_statement=?2 AND peer_encryption=?3", params![wallet, peer.peer_statement, peer.peer_encryption])?;
            for (key, value) in peer.metadata {
                transaction.execute("INSERT INTO paired_host_metadata VALUES (?1,?2,?3,?4,?5)", params![wallet, peer.peer_statement, peer.peer_encryption, key, value])?;
            }
            Ok(())
        })
        .await
    }

    /// Authenticated peer roster with its display metadata.
    pub async fn paired_hosts(&self) -> Result<Vec<PairedHostRecord>, DbError> {
        let wallet = self.owner;
        self.read_records(move |connection| {
            let mut peers = connection
                .prepare(
                    "
                SELECT
                peer_statement,peer_encryption,status,added_at,updated_at,outgoing_update_at,last_sync_offer_id
                FROM paired_hosts WHERE wallet=?1 ORDER BY added_at,peer_statement,peer_encryption
            ",
                )?
                .query_map([wallet], |row| {
                    Ok(PairedHostRecord {
                        peer_statement: row.get(0)?,
                        peer_encryption: row.get(1)?,
                        status: row.get(2)?,
                        added_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        outgoing_update_at: row.get(5)?,
                        last_sync_offer_id: row.get(6)?,
                        metadata: HashMap::new(),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for peer in &mut peers {
                peer.metadata = connection
                    .prepare(
                        "
                SELECT key,value FROM paired_host_metadata WHERE wallet=?1 AND peer_statement=?2 AND
                peer_encryption=?3 ORDER BY key
            ",
                    )?
                    .query_map(params![wallet, peer.peer_statement, peer.peer_encryption], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<Result<_, _>>()?;
            }
            Ok(peers)
        })
        .await
    }

    /// Remove one authenticated pairing and its public replay ledger atomically.
    pub async fn remove_paired_host(
        &self,
        peer_statement: [u8; 32],
        peer_encryption: [u8; 32],
    ) -> Result<(), DbError> {
        let wallet = self.owner;
        self.write_records(move |transaction| {
            transaction.execute("DELETE FROM paired_hosts WHERE wallet=?1 AND peer_statement=?2 AND peer_encryption=?3", params![wallet, peer_statement, peer_encryption])?;
            let key = CoreStorageKey::SsoResponderRequestLedger { root_public_key: wallet, peer_statement_account_id: peer_statement, peer_encryption_public_key: peer_encryption }.encode();
            transaction.execute("DELETE FROM core_state WHERE key=?1", [key])?;
            Ok(())
        })
        .await
    }

    /// Journal an account and its individual slot assignments in one commit.
    pub async fn record_allowance(
        &self,
        mut allowance: AllowanceRecord,
        slots: Vec<StatementSlotRecord>,
    ) -> Result<(), DbError> {
        if slots
            .iter()
            .any(|slot| slot.chain != allowance.chain || slot.account != allowance.account)
        {
            return Err(DbError::InvalidRecord(
                "allowance slots must belong to their account and chain".into(),
            ));
        }
        if !slots.is_empty() {
            allowance.priority = None;
            allowance.last_renewed_period = None;
        }
        let wallet = self.owner;
        self.write_records(move |transaction| {
            let detailed: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM statement_slots WHERE wallet=?1 AND chain=?2 AND account=?3)", params![wallet, allowance.chain, allowance.account], |row| row.get(0))?;
            if detailed {
                allowance.priority = None;
                allowance.last_renewed_period = None;
            }
            transaction.execute(
                "
                INSERT INTO allowance_records VALUES (?1,?2,?3,?4,?5,?6,?7) ON
                CONFLICT(wallet,chain,resource,account) DO UPDATE SET
                priority=excluded.priority,last_renewed_period=excluded.last_renewed_period
            ",
                params![wallet, allowance.chain, allowance.resource, allowance.account, allowance.allocated_at, allowance.priority, allowance.last_renewed_period],
            )?;
            for slot in slots {
                transaction.execute(
                    "
                INSERT INTO statement_slots VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON
                CONFLICT(wallet,chain,collection,period,slot) DO UPDATE SET
                account=excluded.account,priority=CASE WHEN statement_slots.account=excluded.account THEN statement_slots.priority ELSE excluded.priority END,last_allocated_or_renewed_at=excluded.last_allocated_or_renewed_at
            ",
                    params![wallet, slot.chain, slot.collection, slot.period, slot.slot, slot.account, slot.priority, slot.last_allocated_or_renewed_at],
                )?;
            }
            Ok(())
        })
        .await
    }

    /// Allocation history; detailed slot metadata remains authoritative.
    pub async fn allowances(&self) -> Result<Vec<AllowanceRecord>, DbError> {
        let wallet = self.owner;
        self.read_records(move |connection| {
            Ok(connection
                .prepare(
                    "
                SELECT chain,resource,account,allocated_at,priority,last_renewed_period FROM
                allowance_records WHERE wallet=?1 ORDER BY chain,resource,account
            ",
                )?
                .query_map([wallet], |row| {
                    Ok(AllowanceRecord {
                        chain: row.get(0)?,
                        resource: row.get(1)?,
                        account: row.get(2)?,
                        allocated_at: row.get(3)?,
                        priority: row.get(4)?,
                        last_renewed_period: row.get(5)?,
                    })
                })?
                .collect::<Result<_, _>>()?)
        })
        .await
    }

    /// Stop renewing an unpaired account without removing its other resources.
    pub async fn remove_statement_allowance(&self, account: [u8; 32]) -> Result<(), DbError> {
        let wallet = self.owner;
        self.write_records(move |transaction| {
            transaction.execute("DELETE FROM statement_slots WHERE wallet=?1 AND account=?2", params![wallet, account])?;
            transaction.execute("DELETE FROM allowance_records WHERE wallet=?1 AND account=?2 AND resource='statement-store'", params![wallet, account])?;
            Ok(())
        })
        .await
    }

    /// Slots in renewal priority order, retaining each period and timestamp.
    pub async fn statement_slots(&self) -> Result<Vec<StatementSlotRecord>, DbError> {
        let wallet = self.owner;
        self.read_records(move |connection| {
            Ok(connection
                .prepare(
                    "
                SELECT chain,collection,period,slot,account,priority,last_allocated_or_renewed_at FROM
                statement_slots WHERE wallet=?1 ORDER BY priority
                DESC,last_allocated_or_renewed_at DESC,chain,collection,period,slot
            ",
                )?
                .query_map([wallet], |row| Ok(StatementSlotRecord { chain: row.get(0)?, collection: row.get(1)?, period: row.get(2)?, slot: row.get(3)?, account: row.get(4)?, priority: row.get(5)?, last_allocated_or_renewed_at: row.get(6)? }))?
                .collect::<Result<_, _>>()?)
        })
        .await
    }

    /// Move a successfully renewed slot without retaining its previous assignment.
    pub async fn renew_statement_slot(
        &self,
        previous: StatementSlotRecord,
        mut renewed: StatementSlotRecord,
    ) -> Result<(), DbError> {
        if previous.chain != renewed.chain || previous.account != renewed.account {
            return Err(DbError::InvalidRecord(
                "renewed slot must retain its chain and account".into(),
            ));
        }
        renewed.priority = previous.priority;
        let wallet = self.owner;
        self.write_records(move |transaction| {
            let removed=transaction.execute("DELETE FROM statement_slots WHERE wallet=?1 AND chain=?2 AND collection=?3 AND period=?4 AND slot=?5 AND account=?6 AND priority=?7 AND last_allocated_or_renewed_at=?8",params![wallet,previous.chain,previous.collection,previous.period,previous.slot,previous.account,previous.priority,previous.last_allocated_or_renewed_at])?;
            if removed!=1 {return Err(DbError::InvalidRecord("statement slot changed during renewal".into()));}
            transaction.execute("INSERT INTO statement_slots VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![wallet,renewed.chain,renewed.collection,renewed.period,renewed.slot,renewed.account,renewed.priority,renewed.last_allocated_or_renewed_at])?;
            Ok(())
        }).await
    }

    /// Persistent permission decisions, strictly decoded from canonical core records.
    pub async fn permissions(&self) -> Result<Vec<PermissionRecord>, DbError> {
        let store = self.clone();
        self.read_records(move |connection| {
            let mut permissions = Vec::new();
            let rows = connection
                .prepare("SELECT key,value FROM core_state ORDER BY key")?
                .query_map([], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (encoded, envelope) in rows {
                let mut bytes = encoded.as_slice();
                let key = CoreStorageKey::decode(&mut bytes)
                    .map_err(|error| DbError::InvalidRecord(error.to_string()))?;
                if !bytes.is_empty() {
                    return Err(DbError::InvalidRecord(
                        "trailing core storage key bytes".into(),
                    ));
                }
                if let CoreStorageKey::PermissionAuthorization {
                    product_id,
                    request,
                } = key
                {
                    let value = store.decrypt("core_state", &encoded, &envelope)?;
                    let status =
                        crate::host_internal::permissions::decode_persisted_authorization(&value)
                            .map_err(|error| DbError::InvalidRecord(error.reason))?;
                    permissions.push(PermissionRecord {
                        product_id,
                        request,
                        status,
                    });
                }
            }
            Ok(permissions)
        })
        .await
    }

    /// Ensure ownership exists without replacing concurrently refreshed display metadata.
    pub async fn ensure_product(&self, product: String) -> Result<(), DbError> {
        let product = product_id(&product)?;
        self.for_product(&product)?.write_records(move |transaction| {
            transaction.execute("INSERT INTO products(product_id,name) VALUES (?1,?1) ON CONFLICT(product_id) DO NOTHING",[product])?;
            Ok(())
        }).await
    }

    /// Add or update catalog display metadata without changing authorization.
    pub async fn save_product(&self, mut product: ProductRecord) -> Result<(), DbError> {
        product.product_id = product_id(&product.product_id)?;
        self.for_product(&product.product_id)?.write_records(move |transaction| {
            transaction.execute(
                "
                INSERT INTO products VALUES (?1,?2,?3,?4,?5) ON CONFLICT(product_id) DO UPDATE SET
                name=excluded.name,icon_cid=excluded.icon_cid,icon_format=excluded.icon_format,worker_url_override=excluded.worker_url_override
            ",
                params![product.product_id, product.name, product.icon_cid, product.icon_format, product.worker_url_override],
            )?;
            Ok(())
        })
        .await
    }

    /// Observe committed runtime records so native administration can refresh its queries.
    pub fn observe_records(&self) -> BoxStream<'static, Result<(), DbError>> {
        let store = self.clone();
        self.database
            .observe_changes(
                "            SELECT 1 FROM products
            UNION ALL SELECT 1 FROM core_state
            UNION ALL SELECT 1 FROM paired_hosts
            UNION ALL SELECT 1 FROM paired_host_metadata
            UNION ALL SELECT 1 FROM allowance_records
            UNION ALL SELECT 1 FROM statement_slots
            UNION ALL SELECT 1 FROM scheduled_notifications
        ",
            )
            .map(move |result| {
                result?;
                store.ensure_active()
            })
            .boxed()
    }

    /// Current product catalog sorted by normalized identifier.
    pub async fn products(&self) -> Result<Vec<ProductRecord>, DbError> {
        self.read_records(|connection| Ok(connection.prepare("SELECT product_id,name,icon_cid,icon_format,worker_url_override FROM products ORDER BY product_id")?.query_map([], product_row)?.collect::<Result<_, _>>()?)).await
    }

    /// Observe catalog commits across all executions in this account.
    pub fn observe_products(&self) -> BoxStream<'static, Result<Vec<ProductRecord>, DbError>> {
        let store = self.clone();
        self.database
            .observe("SELECT product_id,name,icon_cid,icon_format,worker_url_override FROM products ORDER BY product_id", move |query| {
                store.ensure_active()?;
                query.query_map([], product_row)
            })
            .boxed()
    }

    /// Remove product data and permissions only after OS notifications are gone.
    pub async fn remove_product(&self, product: String) -> Result<(), DbError> {
        let product = product_id(&product)?;
        self.write_records(move |transaction| {
            let pending: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM scheduled_notifications WHERE product_id=?1)",
                [&product],
                |row| row.get(0),
            )?;
            if pending {
                return Err(DbError::InvalidRecord(
                    "cancel product notifications before removing the product".into(),
                ));
            }
            let keys = transaction
                .prepare("SELECT key FROM core_state")?
                .query_map([], |row| row.get::<_, Vec<u8>>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for encoded in keys {
                let mut bytes = encoded.as_slice();
                let key = CoreStorageKey::decode(&mut bytes)
                    .map_err(|error| DbError::InvalidRecord(error.to_string()))?;
                if !bytes.is_empty() {
                    return Err(DbError::InvalidRecord(
                        "trailing core storage key bytes".into(),
                    ));
                }
                let owned = match key {
                    CoreStorageKey::PermissionAuthorization { product_id, .. }
                    | CoreStorageKey::ProductSubtree { product_id, .. }
                    | CoreStorageKey::ProductManifest { product_id } => product_id == product,
                    _ => false,
                };
                if owned {
                    transaction.execute("DELETE FROM core_state WHERE key=?1", [encoded])?;
                }
            }
            transaction.execute(
                "DELETE FROM product_storage WHERE product_id=?1",
                [&product],
            )?;
            transaction.execute("DELETE FROM products WHERE product_id=?1", [&product])?;
            Ok(())
        })
        .await
    }

    /// Persist registration intent before invoking the OS; limits are enforced atomically.
    pub async fn prepare_notification(
        &self,
        product: String,
        text: String,
        deeplink: Option<String>,
        scheduled_at: Option<i64>,
        limit: u32,
    ) -> Result<ScheduledNotificationRecord, DbError> {
        let product = product_id(&product)?;
        self.for_product(&product)?.write_records(move |transaction| {
            let count: u32 = transaction.query_row("SELECT COUNT(*) FROM scheduled_notifications WHERE scheduled_at IS NOT NULL", [], |row| row.get(0))?;
            if scheduled_at.is_some() && count >= limit {
                return Err(DbError::NotificationLimitReached);
            }
            let notification_id = next_id(transaction, "notification")?;
            transaction.execute(
                "
                INSERT INTO
                scheduled_notifications(product_id,notification_id,text,deeplink,scheduled_at,state) VALUES
                (?1,?2,?3,?4,?5,'register')
            ",
                params![product, notification_id, text, deeplink, scheduled_at],
            )?;
            Ok(ScheduledNotificationRecord { product_id: product, notification_id, text, deeplink, scheduled_at, state: NotificationState::Register, revision: 0 })
        })
        .await
    }

    /// Retained payloads and reconciliation work, including failed registrations.
    pub async fn notifications(&self) -> Result<Vec<ScheduledNotificationRecord>, DbError> {
        self.read_records(|connection| {
            Ok(connection
                .prepare(
                    "
                SELECT product_id,notification_id,text,deeplink,scheduled_at,revision,state FROM
                scheduled_notifications ORDER BY notification_id
            ",
                )?
                .query_map([], notification_row)?
                .collect::<Result<_, _>>()?)
        })
        .await
    }

    /// Recover missing OS registrations without losing failed registration or cancellation work.
    pub async fn reconcile_pending_notifications(
        &self,
        pending_ids: Vec<u32>,
        now_ms: i64,
    ) -> Result<(), DbError> {
        self.write_records(move |transaction| {
            let registered = transaction.prepare("SELECT notification_id,scheduled_at FROM scheduled_notifications WHERE state='registered'")?.query_map([], |row| Ok((row.get::<_, u32>(0)?, row.get::<_, Option<i64>>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
            for (notification_id, scheduled_at) in registered {
                if pending_ids.contains(&notification_id) {
                    continue;
                }
                if scheduled_at.is_some_and(|scheduled_at| scheduled_at > now_ms) {
                    transaction.execute("UPDATE scheduled_notifications SET state='register',revision=revision+1 WHERE notification_id=?1", [notification_id])?;
                } else {
                    transaction.execute("DELETE FROM scheduled_notifications WHERE notification_id=?1", [notification_id])?;
                }
            }
            Ok(())
        })
        .await
    }

    /// Persist cancellation intent; retry retains rows until OS success.
    pub async fn cancel_product_notifications(&self, product: String) -> Result<(), DbError> {
        let product = product_id(&product)?;
        self.write_records(move |transaction| {
            transaction.execute(
                "
                UPDATE scheduled_notifications SET state='cancel',revision=revision+1 WHERE product_id=?1
                AND state!='cancel'
            ",
                [product],
            )?;
            Ok(())
        })
        .await
    }

    /// Cancel a single owned notification without touching another product.
    pub async fn cancel_notification(
        &self,
        product: String,
        notification_id: u32,
    ) -> Result<(), DbError> {
        let product = product_id(&product)?;
        self.write_records(move |transaction| {
            transaction.execute(
                "
                UPDATE scheduled_notifications SET state='cancel',revision=revision+1 WHERE product_id=?1
                AND notification_id=?2 AND state!='cancel'
            ",
                params![product, notification_id],
            )?;
            Ok(())
        })
        .await
    }

    /// Apply only an acknowledgment of the exact revision sent to the OS.
    pub async fn acknowledge_notification(
        &self,
        notification_id: u32,
        revision: u64,
        state: NotificationState,
    ) -> Result<bool, DbError> {
        let revision = i64::try_from(revision).map_err(|_| {
            DbError::InvalidRecord("notification revision exceeds SQLite range".into())
        })?;
        self.write_records(move |transaction| {
            let changed = match state {
                NotificationState::Register => {
                    let delivered = transaction.execute("DELETE FROM scheduled_notifications WHERE notification_id=?1 AND revision=?2 AND state='register' AND scheduled_at IS NULL", params![notification_id, revision])?;
                    delivered + transaction.execute("UPDATE scheduled_notifications SET state='registered' WHERE notification_id=?1 AND revision=?2 AND state='register'", params![notification_id, revision])?
                }
                NotificationState::Cancel => transaction.execute("DELETE FROM scheduled_notifications WHERE notification_id=?1 AND revision=?2 AND state='cancel'", params![notification_id, revision])?,
                NotificationState::Registered => return Err(DbError::InvalidRecord("registered notification has no pending OS action".into())),
            };
            Ok(changed == 1)
        })
        .await
    }

    /// Clear account runtime records after cancelling OS notifications.
    pub async fn reset(&self) -> Result<(), DbError> {
        self.write_records(clear_records).await
    }

    /// Delete records and fence queued execution writes in the same database operation.
    pub async fn reset_and_deactivate(&self) -> Result<(), DbError> {
        let active = self.active.clone();
        self.write_records(move |transaction| {
            clear_records(transaction)?;
            active.store(false, core::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .await
    }
}

fn clear_records(transaction: &rusqlite::Transaction<'_>) -> Result<(), DbError> {
    let pending: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM scheduled_notifications)",
        [],
        |row| row.get(0),
    )?;
    if pending {
        return Err(DbError::InvalidRecord(
            "cancel notifications before resetting runtime records".into(),
        ));
    }
    for table in [
        "paired_hosts",
        "allowance_records",
        "statement_slots",
        "products",
        "core_state",
        "product_storage",
    ] {
        transaction.execute(&format!("DELETE FROM {table}"), [])?;
    }
    Ok(())
}
