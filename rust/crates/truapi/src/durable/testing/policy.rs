//! [`ScriptedPolicy`]: a submission policy whose answers a test scripts.

use std::{
	collections::HashMap,
	sync::atomic::{AtomicU8, Ordering},
};

use futures::future::{BoxFuture, pending, ready};
use parking_lot::Mutex;

use super::{extrinsic, insert_with_plan};
use crate::{
	chain::{EncodedExtrinsic, MortalExtrinsic},
	durable::{
		engine::DurableSubmission,
		model::{
			DomainId, DurableTxEntry, DurableTxId, PolicyId, PolicyParams, ScheduledDurableTx,
			SubmissionPlan, VerdictFailureKind,
		},
		policy::{PolicyError, Preparation, SubmissionPolicy},
	},
	store::Db,
};

/// The id of the policy a test uses unless it needs a second one.
pub fn test_policy_id() -> PolicyId {
	PolicyId::new("test-policy")
}

/// A plan of the test policy with `params`.
pub fn test_plan(params: Vec<u8>) -> SubmissionPlan {
	SubmissionPlan { policy: test_policy_id(), params: PolicyParams(params) }
}

/// What a [`ScriptedPolicy`] answers for one batch.
pub type PrepareAnswer = Result<HashMap<DurableTxId, Preparation>, PolicyError>;

/// How a [`ScriptedPolicy`] answers each batch, given the batch's ids.
pub type PrepareScript =
	Box<dyn Fn(&[DurableTxId]) -> BoxFuture<'static, PrepareAnswer> + Send + Sync>;

/// What a [`ScriptedPolicy`] answers to every retry question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetryAnswer {
	Retry,
	Final,
}

/// A policy that answers batches with a script and retries every failure
/// unless told otherwise, recording every question.
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
	pub failure: VerdictFailureKind,
}

impl ScriptedPolicy {
	/// The test policy, answering every batch with `script`.
	pub fn from_script(script: PrepareScript) -> Self {
		Self {
			id: test_policy_id(),
			retry_answer: RetryAnswer::Retry,
			script,
			retry_questions: Mutex::new(Vec::new()),
			batches: Mutex::new(Vec::new()),
		}
	}

	/// The test policy, answering every batch at once with what `answer`
	/// gives for its ids.
	pub fn answering_with(
		answer: impl Fn(&[DurableTxId]) -> PrepareAnswer + Send + Sync + 'static,
	) -> Self {
		Self::from_script(Box::new(move |ids| Box::pin(ready(answer(ids)))))
	}

	/// The test policy, building every row it is asked about into new bytes:
	/// [`nth_build`] for its `n`th build.
	pub fn building_everything() -> Self {
		let builds = AtomicU8::new(0);
		Self::answering_with(move |ids| {
			let answers = ids.iter().map(|id| (*id, Preparation::Ready(next_build(&builds))));
			Ok(answers.collect())
		})
	}

	/// The test policy, with nothing to say about any row yet, so every row
	/// keeps waiting.
	pub fn answering_nothing() -> Self {
		Self::answering_with(|_| Ok(HashMap::new()))
	}

	/// The test policy, giving up on every row it is asked about.
	pub fn giving_up() -> Self {
		Self::answering_with(|ids| answer_all_with(ids, Preparation::GiveUp))
	}

	/// The test policy, whose builds never finish.
	pub fn never_answering() -> Self {
		Self::from_script(Box::new(|_| Box::pin(pending())))
	}

	/// This policy under the id `id`, for a test with a second policy.
	pub fn named(self, id: PolicyId) -> Self {
		Self { id, ..self }
	}

	/// This policy, answering that no failure is worth another attempt.
	pub fn declining_retries(self) -> Self {
		Self { retry_answer: RetryAnswer::Final, ..self }
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

	fn can_retry(
		&self,
		entry: &DurableTxEntry,
		params: &PolicyParams,
		failure: VerdictFailureKind,
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

/// The same `preparation` for every id of `ids`.
pub fn answer_all_with(ids: &[DurableTxId], preparation: Preparation) -> PrepareAnswer {
	Ok(ids.iter().map(|id| (*id, preparation.clone())).collect())
}

/// The bytes the `n`th build of [`ScriptedPolicy::building_everything`]
/// signs, counting from 0.
pub fn nth_build(n: u8) -> MortalExtrinsic {
	extrinsic(100 + n, 100, 64)
}

/// What the first `count` builds of [`ScriptedPolicy::building_everything`]
/// broadcast, in order.
pub fn first_builds(count: u8) -> Vec<EncodedExtrinsic> {
	(0..count).map(|n| nth_build(n).extrinsic).collect()
}

fn next_build(builds: &AtomicU8) -> MortalExtrinsic {
	nth_build(builds.fetch_add(1, Ordering::SeqCst))
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
