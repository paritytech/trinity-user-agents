use std::{collections::VecDeque, sync::Mutex};

use super::*;
use crate::host_internal::bulletin::preimage_key;

/// A source that answers from a script, one entry per call, and counts the calls.
struct Scripted {
	answers: Mutex<VecDeque<Result<Option<Vec<u8>>, SourceError>>>,
	calls: Mutex<u32>,
}

#[async_trait::async_trait]
impl BlobSource for Scripted {
	async fn get(&self, _cid: &BulletinCid) -> Result<Option<Vec<u8>>, SourceError> {
		*self.calls.lock().unwrap() += 1;
		self.answers
			.lock()
			.unwrap()
			.pop_front()
			.expect("scripted source asked more often than scripted")
	}
}

fn blob() -> Vec<u8> {
	b"bulletin lookup test blob".to_vec()
}

fn scripted(answers: Vec<Result<Option<Vec<u8>>, SourceError>>) -> BulletinLookup<Scripted> {
	let source = Scripted { answers: Mutex::new(answers.into()), calls: Mutex::new(0) };
	BulletinLookup {
		source: Arc::new(source),
		timer: Arc::new(RealTimer),
		poll_interval: Duration::from_millis(1),
	}
}

/// Every item of the subscription, errors by their reason.
async fn events(
	lookup: &BulletinLookup<Scripted>,
	key: &[u8; 32],
) -> Vec<Result<Option<Vec<u8>>, String>> {
	lookup
		.lookup_preimage(*key)
		.map(|item| item.map_err(|error| error.reason))
		.collect()
		.await
}

fn calls(lookup: &BulletinLookup<Scripted>) -> u32 {
	*lookup.source.calls.lock().unwrap()
}

fn never() -> Result<Option<Vec<u8>>, SourceError> {
	Err(SourceError::Permanent("invalid CID".to_string()))
}

#[tokio::test]
async fn a_held_blob_is_emitted_at_once_and_the_subscription_ends() {
	let lookup = scripted(vec![Ok(Some(blob()))]);
	assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(Some(blob()))]);
}

#[tokio::test]
async fn a_miss_is_reported_at_once_then_the_blob_when_it_lands_then_the_end() {
	// The caller hears the miss immediately, so it can show "waiting", and is not asked
	// to resubscribe: the same subscription delivers the blob once another host's
	// submission has landed.
	let lookup = scripted(vec![Ok(None), Ok(None), Ok(Some(blob()))]);
	assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(None), Ok(Some(blob()))]);
	assert_eq!(calls(&lookup), 3);
}

/// A node that serves bytes not matching the CID would serve them again, so the lookup asks it
/// no more and hands the bytes over: each caller checks them against its key.
#[tokio::test]
async fn bytes_that_do_not_hash_to_the_key_end_the_lookup_for_the_caller_to_judge() {
	let forged = b"forged".to_vec();
	let lookup = scripted(vec![Ok(None), Ok(Some(forged.clone()))]);
	assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(None), Ok(Some(forged))]);
	assert_eq!(calls(&lookup), 2);
}

#[tokio::test]
async fn a_failing_source_is_a_miss_not_the_end_of_the_subscription() {
	let transient = Err(SourceError::Transient("node down".to_string()));
	let lookup = scripted(vec![transient, Ok(Some(blob()))]);
	assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(None), Ok(Some(blob()))]);
}

/// The caller must learn that the host can never look this up, not see an ordinary end.
#[tokio::test]
async fn a_request_the_node_can_never_answer_ends_the_subscription_with_an_error() {
	let lookup = scripted(vec![never()]);
	assert_eq!(
		events(&lookup, &preimage_key(&blob())).await,
		vec![Ok(None), Err("invalid CID".to_string())]
	);
}

#[tokio::test]
async fn a_request_that_turns_permanent_while_polling_stops_the_poll() {
	let lookup = scripted(vec![Ok(None), never()]);
	assert_eq!(
		events(&lookup, &preimage_key(&blob())).await,
		vec![Ok(None), Err("invalid CID".to_string())]
	);
	assert_eq!(calls(&lookup), 2);
}
