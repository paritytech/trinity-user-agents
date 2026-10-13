//! Unified [`LocalStorage`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::local_storage::{
		HostLocalStorageChangeItem, HostLocalStorageClearError, HostLocalStorageClearRequest,
		HostLocalStorageClearResponse, HostLocalStorageReadError, HostLocalStorageReadRequest,
		HostLocalStorageReadResponse, HostLocalStorageSubscribeError,
		HostLocalStorageSubscribeRequest, HostLocalStorageWriteError, HostLocalStorageWriteRequest,
		HostLocalStorageWriteResponse,
	},
	wire, wire_trait,
};

/// Local key/value storage scoped to the calling product.
#[wire_trait(id = 7)]
#[crate::async_trait]
pub trait LocalStorage: Send + Sync {
	/// Read a value by key.
	///
	/// ```ts
	/// const result = await truapi.localStorage.read({ key: "test-key" });
	/// assert(result.isOk(), "read failed:", result);
	/// console.log("storage value read:", result.value.value);
	/// ```
	#[wire(id = 0)]
	async fn read(
		&self,
		cx: &CallContext,
		request: HostLocalStorageReadRequest,
	) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>>;

	/// Write a value to a key.
	///
	/// ```ts
	/// const result = await truapi.localStorage.write({
	///   key: "test-key",
	///   value: "0x48656c6c6f",
	/// });
	/// assert(result.isOk(), "write failed:", result);
	/// console.log("storage write succeeded");
	/// ```
	#[wire(id = 1)]
	async fn write(
		&self,
		cx: &CallContext,
		request: HostLocalStorageWriteRequest,
	) -> Result<HostLocalStorageWriteResponse, CallError<HostLocalStorageWriteError>>;

	/// Clear a value by key.
	///
	/// ```ts
	/// const result = await truapi.localStorage.clear({ key: "test-key" });
	/// assert(result.isOk(), "clear failed:", result);
	/// console.log("storage clear succeeded");
	/// ```
	#[wire(id = 2)]
	async fn clear(
		&self,
		cx: &CallContext,
		request: HostLocalStorageClearRequest,
	) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>>;

	/// Subscribe to changes of one key in the product's own storage namespace.
	///
	/// Emits the current value immediately, then one item per later write or
	/// clear of the key by any of the product's runtimes. A write that leaves
	/// the stored bytes unchanged emits nothing.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const item = await firstValueFrom(
	///   from(truapi.localStorage.subscribe({ request: { key: "test-key" } })),
	/// );
	/// console.log("storage change received:", item);
	/// ```
	#[wire(id = 3)]
	async fn subscribe(
		&self,
		_cx: &CallContext,
		_request: HostLocalStorageSubscribeRequest,
	) -> Subscription<HostLocalStorageChangeItem, CallError<HostLocalStorageSubscribeError>> {
		Subscription::interrupted(CallError::unavailable())
	}
}
