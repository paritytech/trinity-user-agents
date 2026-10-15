//! Test worker for the sandbox facilities: random bytes, the two clocks,
//! timers in spawned tasks, and a raw call with its own codec.

use core::{cell::RefCell, time::Duration};
use std::rc::Rc;

use futures::channel::oneshot;
use truapi::{
	CallError,
	codec::{Decode, Encode},
	v01,
	versioned::local_storage::{
		HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
	},
};

type ReadResult = Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>>;

#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
	let mut seed = [0u8; 32];
	truapi::random::fill(&mut seed);
	if seed == [0u8; 32] {
		return Err("the host gave no random bytes".into());
	}
	if truapi::time::now_unix_millis() < 1_600_000_000_000 {
		return Err("the host clock is before 2020".into());
	}
	let started = truapi::time::monotonic_millis();

	// The slow task starts first and ends last, so the order of the log
	// shows that each timer wakes only its own task.
	let order = Rc::new(RefCell::new(Vec::new()));
	let (slow_done, slow_finished) = oneshot::channel();
	let slow = order.clone();
	truapi::spawn(async move {
		truapi::time::sleep(Duration::from_millis(40)).await;
		slow.borrow_mut().push("slow");
		let _ = slow_done.send(());
	});
	let fast = order.clone();
	truapi::spawn(async move {
		truapi::time::sleep(Duration::from_millis(10)).await;
		fast.borrow_mut().push("fast");
	});
	// A timer that the worker drops before it elapses.
	drop(truapi::time::sleep(Duration::from_secs(3600)));

	truapi::local_storage::write(v01::HostLocalStorageWriteRequest {
		key: "seed".to_string(),
		value: seed.to_vec(),
	})
	.await?;
	let request = HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest {
		key: "seed".to_string(),
	})
	.encode();
	let payload = truapi::raw::call(truapi::local_storage::imports::read, &request).await;
	let response = ReadResult::decode(&mut &payload[..])?;
	let HostLocalStorageReadResponse::V1(response) = response? else {
		return Err("the host answered a version 1 read in another version".into());
	};
	if response.value.as_deref() != Some(&seed[..]) {
		return Err("the raw read gave another value".into());
	}

	slow_finished.await?;
	let elapsed = truapi::time::monotonic_millis() - started;
	if elapsed < 40 {
		return Err(format!("the slow timer elapsed after only {elapsed} ms").into());
	}
	truapi::log!("order: {}", order.borrow().join(", "));
	Ok(())
}
