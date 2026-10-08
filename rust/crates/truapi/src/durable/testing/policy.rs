//! [`ScriptedPolicy`]: a submission policy whose answers a test scripts.

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;
use parking_lot::Mutex;

use crate::durable::model::{
    DurableTxEntry, DurableTxId, FailureKind, PolicyId, ScheduledDurableTx,
};
use crate::durable::policy::{PolicyError, Preparation, SubmissionPolicy};

/// What a [`ScriptedPolicy`] answers for one batch.
pub type PrepareAnswer = Result<HashMap<DurableTxId, Preparation>, PolicyError>;

/// How a [`ScriptedPolicy`] answers each batch, given the batch's ids.
pub type PrepareScript =
    Box<dyn Fn(&[DurableTxId]) -> BoxFuture<'static, PrepareAnswer> + Send + Sync>;

/// A policy that answers retries with a fixed choice and batches with a
/// script, recording every question.
pub struct ScriptedPolicy {
    id: PolicyId,
    retries: bool,
    script: PrepareScript,
    retry_questions: Mutex<Vec<RetryQuestion>>,
    batches: Mutex<Vec<Vec<DurableTxId>>>,
}

/// One [`SubmissionPolicy::can_retry`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryQuestion {
    /// The entry asked about.
    pub entry: DurableTxEntry,
    /// The params its row stores.
    pub params: Vec<u8>,
    /// Why it failed.
    pub failure: FailureKind,
}

impl ScriptedPolicy {
    /// A policy named `id` that answers `retries` to every retry question and
    /// `script` to every batch.
    pub fn new(id: PolicyId, retries: bool, script: PrepareScript) -> Arc<Self> {
        Arc::new(Self {
            id,
            retries,
            script,
            retry_questions: Mutex::new(Vec::new()),
            batches: Mutex::new(Vec::new()),
        })
    }

    /// A policy that answers `retries` and never builds anything.
    pub fn retrying(id: PolicyId, retries: bool) -> Arc<Self> {
        Self::new(
            id,
            retries,
            Box::new(|_| Box::pin(futures::future::pending())),
        )
    }

    /// Every retry question asked, in order.
    pub fn retry_questions(&self) -> Vec<RetryQuestion> {
        self.retry_questions.lock().clone()
    }

    /// The ids of every batch asked, in order.
    pub fn batches(&self) -> Vec<Vec<DurableTxId>> {
        self.batches.lock().clone()
    }
}

#[async_trait::async_trait]
impl SubmissionPolicy for ScriptedPolicy {
    fn id(&self) -> PolicyId {
        self.id.clone()
    }

    async fn can_retry(&self, entry: &DurableTxEntry, params: &[u8], failure: FailureKind) -> bool {
        let question = RetryQuestion {
            entry: entry.clone(),
            params: params.to_vec(),
            failure,
        };
        self.retry_questions.lock().push(question);
        self.retries
    }

    async fn prepare(&self, batch: &[ScheduledDurableTx]) -> PrepareAnswer {
        let ids: Vec<DurableTxId> = batch.iter().map(|scheduled| scheduled.id).collect();
        self.batches.lock().push(ids.clone());
        (self.script)(&ids).await
    }
}
