use std::{collections::VecDeque, sync::Mutex};

use truapi::preimage_key;

use super::*;

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

fn scripted(answers: Vec<Result<Option<Vec<u8>>, SourceError>>) -> Arc<BulletinLookup<Scripted>> {
	let source = Scripted { answers: Mutex::new(answers.into()), calls: Mutex::new(0) };
	Arc::new(BulletinLookup::with_poll_interval(source, Duration::from_millis(1)))
}

/// Every item of the subscription, errors by their reason.
async fn events(
	lookup: &Arc<BulletinLookup<Scripted>>,
	key: &[u8; 32],
) -> Vec<Result<Option<Vec<u8>>, String>> {
	lookup
		.subscribe(*key)
		.map(|item| item.map_err(|error| error.reason))
		.collect()
		.await
}

fn calls(lookup: &Arc<BulletinLookup<Scripted>>) -> u32 {
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
	// The product hears the miss immediately, so it can show "waiting", and is not asked
	// to resubscribe: the same subscription delivers the blob once another host's
	// submission has landed.
	let lookup = scripted(vec![Ok(None), Ok(None), Ok(Some(blob()))]);
	assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(None), Ok(Some(blob()))]);
	assert_eq!(calls(&lookup), 3);
}

#[tokio::test]
async fn bytes_that_do_not_hash_to_the_key_are_never_emitted() {
	let lookup = scripted(vec![Ok(Some(b"forged".to_vec())), Ok(Some(blob()))]);
	assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(None), Ok(Some(blob()))]);
}

#[tokio::test]
async fn a_failing_source_is_a_miss_not_the_end_of_the_subscription() {
	let transient = Err(SourceError::Transient("node down".to_string()));
	let lookup = scripted(vec![transient, Ok(Some(blob()))]);
	assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(None), Ok(Some(blob()))]);
}

/// The product must learn that the host can never look this up, not see an ordinary end.
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
