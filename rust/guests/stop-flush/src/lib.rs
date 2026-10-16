//! Test worker for the stop of the host. It counts in memory, and its stop
//! handler writes the count to the product storage. The handler first waits
//! on a timer, so that the host must answer calls of the worker after the
//! stop.
//!
//! The first value of the product storage key `hold` makes the handler wait
//! for ever, so that the stop ends at the end of the grace time.

use core::{cell::Cell, time::Duration};
use std::rc::Rc;

use truapi::{
	CallError,
	codec::{Decode, Encode},
	latest::HostLocalStorageWriteRequest,
	v01,
	versioned::local_storage::{
		HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
	},
};

type ReadResult = Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>>;

#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
	// A version 1 read, which every host answers.
	let request = HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest {
		key: "hold".to_string(),
	})
	.encode();
	let payload = truapi::raw::call(truapi::local_storage::imports::read, &request).await;
	let hold = match ReadResult::decode(&mut &payload[..])?? {
		HostLocalStorageReadResponse::V1(response) => response.value.is_some(),
		HostLocalStorageReadResponse::V2(response) => response.value.is_some(),
	};
	let count = Rc::new(Cell::new(0u32));
	let saved = count.clone();
	truapi::on_stop(move || async move {
		truapi::log!("stopping");
		truapi::time::sleep(Duration::from_millis(20)).await;
		if hold {
			core::future::pending::<()>().await;
		}
		let written = truapi::local_storage::write(HostLocalStorageWriteRequest {
			key: "count".to_string(),
			value: saved.get().to_le_bytes().to_vec(),
		})
		.await;
		match written {
			Ok(_) => truapi::log!("flushed {}", saved.get()),
			Err(error) => truapi::log!("flush failed: {error:?}"),
		}
	});
	truapi::log!("running");
	loop {
		truapi::time::sleep(Duration::from_millis(5)).await;
		count.set(count.get() + 1);
	}
}
