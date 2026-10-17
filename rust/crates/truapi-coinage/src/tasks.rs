// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use futures::future::{AbortHandle, Abortable};
use std::collections::HashMap;
use std::future::Future;
use {parking_lot::Mutex, std::sync::Arc};

/// Launch a cancellable future on the Host's executor.
pub(crate) fn spawn_abortable<F>(spawner: &crate::Spawner, work: F) -> AbortHandle
where
    F: Future<Output = ()> + Send + 'static,
{
    let (abort, registration) = AbortHandle::new_pair();
    spawner(Box::pin(async move {
        let _ = Abortable::new(work, registration).await;
    }));
    abort
}

/// Per-message task ownership with cancellation and duplicate suppression.
pub struct ActiveTaskRegistry {
    spawner: crate::Spawner,
    state: Mutex<RegistryState>,
}

#[derive(Default)]
struct RegistryState {
    next_generation: u64,
    tasks: HashMap<String, (u64, AbortHandle)>,
}

impl ActiveTaskRegistry {
    /// Bind background work to the owning Host executor.
    pub fn new(spawner: crate::Spawner) -> Self {
        Self {
            spawner,
            state: Mutex::new(RegistryState::default()),
        }
    }

    /// Start only if this message does not already have a live task.
    pub fn try_start<F>(self: &Arc<Self>, message_id: &str, work: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut state = self.state.lock();
        if state.tasks.contains_key(message_id) {
            return false;
        }
        let generation = state.next_generation;
        state.next_generation = generation.checked_add(1).expect("task generation overflow");
        let (abort, registration) = AbortHandle::new_pair();
        let id = message_id.to_owned();
        state.tasks.insert(id.clone(), (generation, abort));
        drop(state);
        // Construct outside the future so cancellation before its first poll
        // still releases the id. An older cancelled task cannot remove a restart.
        let guard = RemoveOnDrop {
            registry: Arc::clone(self),
            id,
            generation,
        };
        (self.spawner)(Box::pin(
            Abortable::new(
                async move {
                    let _guard = guard;
                    work.await;
                },
                registration,
            )
            .map(|_| ()),
        ));
        true
    }

    /// Whether work currently owns this message id.
    pub fn is_tracked(&self, message_id: &str) -> bool {
        self.state.lock().tasks.contains_key(message_id)
    }

    /// Number of live message tasks.
    pub fn len(&self) -> usize {
        self.state.lock().tasks.len()
    }

    /// Whether no message is currently owned.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cancel all work and make the message ids available for restart.
    pub fn cancel_all(&self) {
        let handles: Vec<_> = self
            .state
            .lock()
            .tasks
            .drain()
            .map(|(_, (_, handle))| handle)
            .collect();
        for handle in handles {
            handle.abort();
        }
    }
}

use futures::FutureExt;

struct RemoveOnDrop {
    registry: Arc<ActiveTaskRegistry>,
    id: String,
    generation: u64,
}

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let mut state = self.registry.state.lock();
        if state
            .tasks
            .get(&self.id)
            .is_some_and(|(generation, _)| *generation == self.generation)
        {
            state.tasks.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::watch;

    use super::*;

    async fn settle(registry: &ActiveTaskRegistry, until: impl Fn(&ActiveTaskRegistry) -> bool) {
        for _ in 0..1_000 {
            if until(registry) {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("registry never settled");
    }

    #[tokio::test]
    async fn duplicate_starts_are_rejected_until_completion() {
        let registry = Arc::new(ActiveTaskRegistry::new(crate::test_spawner()));
        let (release_tx, release_rx) = watch::channel(false);
        let runs = Arc::new(AtomicUsize::new(0));

        let work = {
            let runs = Arc::clone(&runs);
            let mut release = release_rx.clone();
            async move {
                runs.fetch_add(1, Ordering::SeqCst);
                while !*release.borrow() {
                    if release.changed().await.is_err() {
                        return;
                    }
                }
            }
        };
        assert!(registry.try_start("m1", work));
        assert!(registry.is_tracked("m1"));
        assert!(
            !registry.try_start("m1", async {}),
            "in-flight id rejects a second task"
        );

        release_tx.send(true).unwrap();
        settle(&registry, |r| !r.is_tracked("m1")).await;
        assert_eq!(runs.load(Ordering::SeqCst), 1, "the duplicate never ran");
        assert!(registry.try_start("m1", async {}), "restart after defer");
    }

    #[tokio::test]
    async fn distinct_ids_run_concurrently() {
        let registry = Arc::new(ActiveTaskRegistry::new(crate::test_spawner()));
        let (_release_tx, release_rx) = watch::channel(false);
        for id in ["a", "b", "c"] {
            let mut release = release_rx.clone();
            assert!(registry.try_start(id, async move {
                let _ = release.changed().await;
            }));
        }
        assert_eq!(registry.len(), 3);
    }

    #[tokio::test]
    async fn cancel_all_aborts_and_clears() {
        let registry = Arc::new(ActiveTaskRegistry::new(crate::test_spawner()));
        let completed = Arc::new(AtomicUsize::new(0));
        for id in ["a", "b"] {
            let completed = Arc::clone(&completed);
            registry.try_start(id, async move {
                std::future::pending::<()>().await;
                completed.fetch_add(1, Ordering::SeqCst);
            });
        }
        registry.cancel_all();
        settle(&registry, |r| r.is_empty()).await;
        assert_eq!(completed.load(Ordering::SeqCst), 0, "aborted, not run");
        assert!(registry.try_start("a", async {}), "ids are free again");
    }

    #[tokio::test]
    async fn cancelled_unpolled_task_cannot_remove_its_replacement() {
        let queue = Arc::new(Mutex::new(
            Vec::<futures::future::BoxFuture<'static, ()>>::new(),
        ));
        let pending = Arc::clone(&queue);
        let registry = Arc::new(ActiveTaskRegistry::new(Arc::new(move |future| {
            pending.lock().push(future);
        })));
        assert!(registry.try_start("message", std::future::pending()));
        registry.cancel_all();
        assert!(registry.try_start("message", std::future::pending()));
        let old = queue.lock().remove(0);
        old.await;
        assert!(registry.is_tracked("message"));
        assert!(!registry.try_start("message", async {}));
        registry.cancel_all();
        let replacement = queue.lock().remove(0);
        replacement.await;
        assert!(registry.is_empty());
    }
}
