use std::sync::Arc;

use crate::latest::{GenericError, HostPushNotificationRequest, HostPushNotificationError};
use crate::store::{NotificationState, RuntimeStore, ScheduledNotificationRecord};

use super::callbacks::HostCallbacks;
use super::storage::NativeStorage;

/// Serial OS reconciliation for the active account's durable notification queue.
pub struct NativeNotifications {
    storage: Arc<NativeStorage>,
    callbacks: Arc<dyn HostCallbacks>,
    gate: Arc<futures::lock::Mutex<()>>,
}

impl NativeNotifications {
    /// Share one queue between product executions and native administration.
    pub fn new(storage: Arc<NativeStorage>, callbacks: Arc<dyn HostCallbacks>) -> Self {
        Self { storage, callbacks, gate: Arc::new(futures::lock::Mutex::new(())) }
    }

    /// Bind an execution to its activation while sharing serial OS reconciliation.
    pub fn scoped(&self, storage: Arc<NativeStorage>) -> Self {
        Self { storage, callbacks: self.callbacks.clone(), gate: self.gate.clone() }
    }

    /// Persist registration intent before asking the OS to schedule delivery.
    pub async fn schedule(&self, product: String, request: HostPushNotificationRequest) -> Result<u32, HostPushNotificationError> {
        let _gate = self.gate.lock().await;
        let store = self.storage.current().map_err(|error| notification_error(error.reason))?;
        self.reconcile_pending(&store).await.map_err(|error| notification_error(error.reason))?;
        let scheduled_at = request.scheduled_at.map(i64::try_from).transpose().map_err(notification_error)?;
        store.ensure_product(product.clone()).await.map_err(notification_error)?;
        let record = store.prepare_notification(product, request.text, request.deeplink, scheduled_at, 64).await.map_err(|error| match error {
            crate::store::DbError::NotificationLimitReached => HostPushNotificationError::ScheduleLimitReached,
            other => notification_error(other),
        })?;
        self.apply(&store, &record).await.map_err(|error| notification_error(error.reason))?;
        Ok(record.notification_id)
    }

    /// Retain a cancellation until the OS has acknowledged it.
    pub async fn cancel(&self, product: String, id: u32) -> Result<(), GenericError> {
        let _gate = self.gate.lock().await;
        let store = self.storage.current()?;
        store.cancel_notification(product, id).await.map_err(error)?;
        self.reconcile_store(&store).await
    }

    /// Cancel all product notifications before deleting the product's records.
    pub async fn cancel_product(&self, product: String) -> Result<(), GenericError> {
        let _gate = self.gate.lock().await;
        let store = self.storage.current()?;
        store.cancel_product_notifications(product).await.map_err(error)?;
        self.reconcile_store(&store).await
    }

    /// Retry durable registration and cancellation intents after interruption.
    pub async fn reconcile(&self) -> Result<(), GenericError> {
        let _gate = self.gate.lock().await;
        let store = self.storage.current()?;
        self.reconcile_pending(&store).await?;
        self.reconcile_store(&store).await
    }

    async fn reconcile_pending(&self, store: &RuntimeStore) -> Result<(), GenericError> {
        let mut pending = Vec::new();
        for record in store.notifications().await.map_err(error)? {
            if self.callbacks.is_scheduled_notification_pending(record.product_id, record.notification_id).await.map_err(GenericError::from)? {
                pending.push(record.notification_id);
            }
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(error)?.as_millis();
        store.reconcile_pending_notifications(pending, i64::try_from(now).map_err(error)?).await.map_err(error)
    }

    /// Stop delivery while another wallet is active, retaining this account's queue.
    pub async fn suspend(&self) -> Result<(), GenericError> {
        let _gate = self.gate.lock().await;
        let store = self.storage.current()?;
        for record in store.notifications().await.map_err(error)? {
            self.callbacks.cancel_scheduled_notification(record.product_id, record.notification_id).await.map_err(GenericError::from)?;
        }
        Ok(())
    }

    async fn reconcile_store(&self, store: &RuntimeStore) -> Result<(), GenericError> {
        for record in store.notifications().await.map_err(error)? {
            self.apply(store, &record).await?;
        }
        Ok(())
    }

    async fn apply(&self, store: &RuntimeStore, record: &ScheduledNotificationRecord) -> Result<(), GenericError> {
        match record.state {
            NotificationState::Register => self.callbacks.schedule_notification(
                record.product_id.clone(),
                record.notification_id,
                HostPushNotificationRequest {
                    text: record.text.clone(),
                    deeplink: record.deeplink.clone(),
                    scheduled_at: record.scheduled_at.map(u64::try_from).transpose().map_err(error)?,
                },
            ).await.map_err(GenericError::from)?,
            NotificationState::Cancel => self.callbacks.cancel_scheduled_notification(record.product_id.clone(), record.notification_id).await.map_err(GenericError::from)?,
            NotificationState::Registered => return Ok(()),
        }
        if !store.acknowledge_notification(record.notification_id, record.revision, record.state).await.map_err(error)? {
            return Err(error("notification changed during OS reconciliation"));
        }
        Ok(())
    }
}

fn error(reason: impl ToString) -> GenericError {
    GenericError { reason: reason.to_string() }
}

fn notification_error(reason: impl ToString) -> HostPushNotificationError {
    HostPushNotificationError::Unknown { reason: reason.to_string() }
}

#[cfg(test)]
mod tests;
