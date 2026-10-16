//! [`ScriptedPolicy`]: a submission policy whose answers a test scripts,
//! and the fixtures for rows that name a policy.

use std::{collections::HashMap, sync::Arc};

use futures::future::BoxFuture;
use parking_lot::Mutex;
use tempfile::TempDir;

use super::{FakeChain, GENESIS, ManualTimer, engine_with, insert_with_plan};
use crate::{
	chain::MortalExtrinsic,
	durable::{
		dao,
		engine::{DurableSubmission, DurableTxEngine},
		model::{
			DomainId, DurableTxEntry, DurableTxId, FailureKind, PolicyId, PolicyParams,
			ScheduledDurableTx, SubmissionPlan,
		},
		oracle::{DurableRegistry, Unobservable},
		policy::{PolicyError, Preparation, SubmissionPolicy},
	},
	store::Db,
};

/// The policy most tests register.
pub const REBUILD_POLICY: PolicyId = PolicyId::from_static("rebuild");

/// What a [`ScriptedPolicy`] answers for one batch.
pub type PrepareAnswer = Result<HashMap<DurableTxId, Preparation>, PolicyError>;

/// How a [`ScriptedPolicy`] answers each batch, given the batch's ids.
pub type PrepareScript =
	Box<dyn Fn(&[DurableTxId]) -> BoxFuture<'static, PrepareAnswer> + Send + Sync>;

/// What a [`ScriptedPolicy`] answers to every retry question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryAnswer {
	/// The failed transaction is built again.
	Retry,
	/// The failure is final.
	Final,
}

/// A policy that answers retries with a fixed choice and batches with a
/// script, recording every question.
pub struct ScriptedPolicy {
	id: PolicyId,
	retry_answer: RetryAnswer,
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
	pub params: PolicyParams,
	/// Why it failed.
	pub failure: FailureKind,
}

impl ScriptedPolicy {
	/// A policy named `id` that answers `retry_answer` to every retry
	/// question and `script` to every batch.
	pub fn new(id: PolicyId, retry_answer: RetryAnswer, script: PrepareScript) -> Arc<Self> {
		Arc::new(Self {
			id,
			retry_answer,
			script,
			retry_questions: Mutex::new(Vec::new()),
			batches: Mutex::new(Vec::new()),
		})
	}

	/// A policy that answers `retry_answer` and never builds anything.
	pub fn never_building(id: PolicyId, retry_answer: RetryAnswer) -> Arc<Self> {
		let script: PrepareScript = Box::new(|_| Box::pin(futures::future::pending()));
		Self::new(id, retry_answer, script)
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

	async fn can_retry(
		&self,
		entry: &DurableTxEntry,
		params: &PolicyParams,
		failure: FailureKind,
	) -> bool {
		let question = RetryQuestion { entry: entry.clone(), params: params.clone(), failure };
		self.retry_questions.lock().push(question);
		self.retry_answer == RetryAnswer::Retry
	}

	async fn prepare(&self, batch: &[ScheduledDurableTx]) -> PrepareAnswer {
		let ids: Vec<DurableTxId> = batch.iter().map(|scheduled| scheduled.id).collect();
		self.batches.lock().push(ids.clone());
		(self.script)(&ids).await
	}
}

/// A plan of [`REBUILD_POLICY`] with `params`.
pub fn rebuild_plan(params: Vec<u8>) -> SubmissionPlan {
	SubmissionPlan { policy: REBUILD_POLICY, params: PolicyParams(params) }
}

/// `extrinsics` as presigned submissions with no plan to rebuild them.
pub fn without_plan(extrinsics: Vec<MortalExtrinsic>) -> Vec<DurableSubmission> {
	extrinsics
		.into_iter()
		.map(|extrinsic| DurableSubmission { extrinsic, plan: None })
		.collect()
}

/// Records `extrinsic` as a pending row of `domain` rebuilt through `plan`,
/// without watching it.
pub fn insert_retryable(
	db: &Db,
	domain: &DomainId,
	extrinsic: MortalExtrinsic,
	plan: SubmissionPlan,
) -> DurableTxId {
	insert_with_plan(db, domain, extrinsic, Some(plan))
}

/// A row of `domain` waiting to be built through `plan`, written as a
/// previous process would have left it: no executor hears of it.
pub fn leave_waiting(db: &Db, domain: DomainId, plan: SubmissionPlan) -> DurableTxId {
	let write = db.write(move |tx| dao::schedule(tx, &domain, None, &plan));
	futures::executor::block_on(write).unwrap()
}

/// An engine over `chain` serving `domains`, which nothing on chain decides,
/// with `policies`.
pub fn engine_with_policies(
	chain: &Arc<FakeChain>,
	domains: &[DomainId],
	policies: Vec<Arc<ScriptedPolicy>>,
) -> (TempDir, Arc<DurableTxEngine>, Arc<ManualTimer>) {
	let mut registry = DurableRegistry::new();
	for domain in domains {
		registry = registry.with_domain(domain.clone(), Arc::new(Unobservable(GENESIS)));
	}
	for policy in policies {
		registry = registry.with_policy(policy);
	}
	engine_with(chain, registry)
}
