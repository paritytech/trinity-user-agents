//! Unified [`Permissions`] trait.

use crate::{
	CallContext, CallError,
	versioned::permissions::{
		HostDevicePermissionError, HostDevicePermissionRequest, HostDevicePermissionResponse,
		RemotePermissionError, RemotePermissionRequest, RemotePermissionResponse,
	},
	wire, wire_trait,
};

/// Permission request methods.
#[wire_trait(id = 10)]
#[crate::async_trait]
pub trait Permissions: Send + Sync {
	/// Request a device-capability permission from the user.
	///
	/// ```ts
	/// const result = await truapi.permissions.requestDevicePermission("Camera");
	/// assert(result.isOk(), "requestDevicePermission failed:", result);
	/// console.log("device permission result:", result.value);
	/// ```
	#[wire(id = 0)]
	async fn request_device_permission(
		&self,
		cx: &CallContext,
		request: HostDevicePermissionRequest,
	) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>>;

	/// Request a remote-operation permission.
	///
	/// This example makes live requests to Frankfurter after permission is granted.
	///
	/// ```ts
	/// const result = await truapi.permissions.requestRemotePermission({
	///   permission: { tag: "Remote", value: { domains: ["api.frankfurter.dev"] } },
	/// });
	/// assert(result.isOk(), "requestRemotePermission failed:", result);
	/// console.log("remote permission result:", result.value);
	/// if (result.value.granted) {
	///   const response = await fetch("https://api.frankfurter.dev/v2/rates?base=EUR&quotes=USD");
	///   assert(response.ok, "Fetch after permission grant failed:", response.status);
	///   const rates = await response.json();
	///   assert(Array.isArray(rates) && rates.length > 0, "Expected exchange rates:", rates);
	///   console.log("exchange rates:", rates);
	///
	///   const xhrRates = await new Promise((resolve, reject) => {
	///     const request = new XMLHttpRequest();
	///     request.open("GET", "https://api.frankfurter.dev/v2/rates?base=EUR&quotes=USD");
	///     request.responseType = "json";
	///     request.timeout = 15000;
	///     request.onload = () => request.status === 200
	///       ? resolve(request.response)
	///       : reject(new Error(`XHR failed: ${request.status}`));
	///     request.onerror = request.ontimeout = () => reject(new Error("XHR failed"));
	///     request.send();
	///   });
	///   assert(Array.isArray(xhrRates) && xhrRates.length > 0, "Expected XHR exchange rates:", xhrRates);
	///   console.log("XHR exchange rates:", xhrRates);
	/// } else {
	///   console.log("Remote permission denied; skipping network requests.");
	/// }
	/// ```
	#[wire(id = 1)]
	async fn request_remote_permission(
		&self,
		cx: &CallContext,
		request: RemotePermissionRequest,
	) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>>;

	/// Authorize one remote operation, consuming an available one-use grant.
	#[wire(id = 2, internal)]
	async fn authorize_remote_permission(
		&self,
		cx: &CallContext,
		request: RemotePermissionRequest,
	) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>>;

	/// Authorize one device operation, consuming an available one-use grant.
	#[wire(id = 3, internal)]
	async fn authorize_device_permission(
		&self,
		cx: &CallContext,
		request: HostDevicePermissionRequest,
	) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>>;
}
