use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::future::{AbortHandle, Abortable};
use futures::stream::BoxStream;

use crate::PermissionRecord;
use crate::latest::GenericError;
use crate::subscription::Spawner;

/// Receives committed saved permission snapshots, independently of wallet lock.
#[uniffi::export(callback_interface)]
pub trait NativePermissionObserver: Send + Sync {
    /// Replace the saved permission records for the selected scope.
    fn on_update(&self, records: Vec<PermissionRecord>);
    /// Report that observation ended.
    fn on_complete(&self);
    /// Report a failed read without presenting an empty permission list.
    fn on_error(&self, reason: String);
}

/// Observation ends when its native owner releases or cancels this handle.
#[derive(uniffi::Object)]
pub struct NativePermissionSubscription {
    abort: Mutex<Option<AbortHandle>>,
}

#[uniffi::export]
impl NativePermissionSubscription {
    /// Stop delivering permission snapshots.
    pub fn cancel(&self) {
        if let Some(abort) = self
            .abort
            .lock()
            .expect("permission subscription mutex poisoned")
            .take()
        {
            abort.abort();
        }
    }
}

impl Drop for NativePermissionSubscription {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Bind the real repository stream to its native observation lifetime.
pub fn observe_permissions(
    mut stream: BoxStream<'static, Result<Vec<PermissionRecord>, GenericError>>,
    observer: Arc<dyn NativePermissionObserver>,
    spawner: Spawner,
) -> Arc<NativePermissionSubscription> {
    let (abort, registration) = AbortHandle::new_pair();
    spawner(Box::pin(async move {
        let _ = Abortable::new(
            async move {
                while let Some(records) = stream.next().await {
                    match records {
                        Ok(records) => observer.on_update(records),
                        Err(error) => {
                            observer.on_error(error.reason);
                            return;
                        }
                    }
                }
                observer.on_complete();
            },
            registration,
        )
        .await;
    }));
    Arc::new(NativePermissionSubscription {
        abort: Mutex::new(Some(abort)),
    })
}
