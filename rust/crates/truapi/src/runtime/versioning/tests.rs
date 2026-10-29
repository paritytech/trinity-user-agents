use super::*;
use crate::{
	latest, v01,
	versioned::local_storage::{
		HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
	},
};
use futures::{FutureExt, executor::block_on, stream};
use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};

struct ReadCase {
	request: HostLocalStorageReadRequest,
	response: HostLocalStorageReadResponse,
	refusal: HostLocalStorageReadError,
}

fn read_cases() -> [ReadCase; 2] {
	let response = latest::HostLocalStorageReadResponse { value: Some(b"key".to_vec()) };
	[
		ReadCase {
			request: HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest {
				key: "key".to_string(),
			}),
			response: HostLocalStorageReadResponse::V1(response.clone()),
			refusal: HostLocalStorageReadError::V1(v01::HostLocalStorageReadError::Unknown {
				reason: latest::HostLocalStorageReadError::AccessNotGranted.to_string(),
			}),
		},
		ReadCase {
			request: HostLocalStorageReadRequest::V2(latest::HostLocalStorageReadRequest {
				product: None,
				key: "key".to_string(),
			}),
			response: HostLocalStorageReadResponse::V2(response),
			refusal: HostLocalStorageReadError::V2(
				latest::HostLocalStorageReadError::AccessNotGranted,
			),
		},
	]
}

#[test]
fn latest_responses_return_in_the_request_version() {
	for case in read_cases() {
		let response: Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>> =
			block_on(version(case.request).exec(|request| async move {
				Ok(latest::HostLocalStorageReadResponse { value: Some(request.key.into_bytes()) })
			}));
		assert_eq!(response, Ok(case.response));
	}
}

#[test]
fn early_returns_and_question_mark_errors_use_the_request_version() {
	for early_return in [true, false] {
		for case in read_cases() {
			let response: Result<
				HostLocalStorageReadResponse,
				CallError<HostLocalStorageReadError>,
			> = block_on(version(case.request).exec(|_| async move {
				let refusal =
					CallError::Domain(latest::HostLocalStorageReadError::AccessNotGranted);
				if early_return {
					return Err(refusal);
				}
				Err::<(), _>(refusal)?;
				Ok(latest::HostLocalStorageReadResponse { value: None })
			}));
			assert_eq!(response, Err(CallError::Domain(case.refusal)));
		}
	}
}

#[test]
fn subscription_items_and_later_interruptions_use_the_request_version() {
	for case in read_cases() {
		let subscription: Subscription<
			HostLocalStorageReadResponse,
			CallError<HostLocalStorageReadError>,
		> = block_on(version(case.request).exec(|request| async move {
			Subscription::new(stream::iter([
				Ok(latest::HostLocalStorageReadResponse { value: Some(request.key.into_bytes()) }),
				Err(CallError::Domain(latest::HostLocalStorageReadError::AccessNotGranted)),
			]))
		}));
		assert_eq!(
			block_on(subscription.collect::<Vec<_>>()),
			vec![Ok(case.response), Err(CallError::Domain(case.refusal))],
		);
	}
}

#[test]
fn immediate_subscription_interruptions_use_the_request_version() {
	for case in read_cases() {
		let subscription: Subscription<
			HostLocalStorageReadResponse,
			CallError<HostLocalStorageReadError>,
		> = block_on(version(case.request).exec(|_| async {
			Subscription::interrupted(CallError::Domain(
				latest::HostLocalStorageReadError::AccessNotGranted,
			))
		}));
		assert_eq!(
			block_on(subscription.collect::<Vec<_>>()),
			vec![Err(CallError::Domain(case.refusal))],
		);
	}
}

fn framework_errors<Error>() -> [CallError<Error>; 5] {
	[
		CallError::Denied,
		CallError::Unsupported,
		CallError::MalformedFrame { reason: "malformed".to_string() },
		CallError::HostFailure { reason: "offline".to_string() },
		CallError::Cancelled,
	]
}

#[test]
fn framework_failures_keep_their_variant_and_reason() {
	for early_return in [true, false] {
		for case in read_cases() {
			for (error, expected) in framework_errors().into_iter().zip(framework_errors()) {
				let response: Result<
					HostLocalStorageReadResponse,
					CallError<HostLocalStorageReadError>,
				> = block_on(version(case.request.clone()).exec(|_| async move {
					if early_return {
						return Err(error);
					}
					Err::<(), _>(error)?;
					Ok(latest::HostLocalStorageReadResponse { value: None })
				}));
				assert_eq!(response, Err(expected));
			}
		}
	}
}

#[test]
fn framework_interruptions_keep_their_variant_and_reason() {
	for case in read_cases() {
		for (error, expected) in framework_errors().into_iter().zip(framework_errors()) {
			let subscription: Subscription<
				HostLocalStorageReadResponse,
				CallError<HostLocalStorageReadError>,
			> = block_on(
				version(case.request.clone()).exec(|_| async { Subscription::interrupted(error) }),
			);
			assert_eq!(block_on(subscription.collect::<Vec<_>>()), vec![Err(expected)]);
		}
	}
}

struct DropSignal(Arc<AtomicBool>);

impl Drop for DropSignal {
	fn drop(&mut self) {
		self.0.store(true, Ordering::SeqCst);
	}
}

#[test]
fn converting_a_subscription_keeps_it_lazy_and_drops_its_source() {
	let polled = Arc::new(AtomicBool::new(false));
	let dropped = Arc::new(AtomicBool::new(false));
	let source_polled = polled.clone();
	let source_guard = DropSignal(dropped.clone());
	let mut subscription: Subscription<
		HostLocalStorageReadResponse,
		CallError<HostLocalStorageReadError>,
	> = block_on(version(read_cases()[0].request.clone()).exec(|_| async move {
		Subscription::new(stream::once(async move {
			let _guard = source_guard;
			source_polled.store(true, Ordering::SeqCst);
			core::future::pending().await
		}))
	}));
	assert!(!polled.load(Ordering::SeqCst));
	assert_eq!(subscription.next().now_or_never(), None);
	assert!(polled.load(Ordering::SeqCst));
	assert!(!dropped.load(Ordering::SeqCst));
	drop(subscription);
	assert!(dropped.load(Ordering::SeqCst));
}
