//! One conversion boundary around each product-facing runtime method.

use core::future::Future;

use futures::StreamExt;

use crate::{
	CallError, Subscription,
	versioned::{FromLatest, IntoLatest},
};

/// A latest request paired with the version its caller understands.
pub struct VersionedRequest<Request> {
	request: Request,
	version: u8,
}

/// Capture the caller's version before normalizing its request.
pub fn version<Request: IntoLatest>(request: Request) -> VersionedRequest<Request::Latest> {
	VersionedRequest { version: request.version(), request: request.into_latest() }
}

impl<Request> VersionedRequest<Request> {
	/// Convert every handler outcome, including early exits, to the caller's version.
	pub async fn exec<Output: VersionedOutput, Fut: Future<Output = Output::Latest>>(
		self,
		handler: impl FnOnce(Request) -> Fut,
	) -> Output {
		Output::from_latest(handler(self.request).await, self.version)
	}
}

/// A method result whose payloads can be returned in the caller's version.
pub trait VersionedOutput {
	/// The handler's output before version envelopes are applied.
	type Latest;

	/// Wrap every response and domain error in the requested version.
	fn from_latest(latest: Self::Latest, version: u8) -> Self;
}

impl<Response: FromLatest, Error: FromLatest> VersionedOutput
	for Result<Response, CallError<Error>>
{
	type Latest = Result<Response::Latest, CallError<Error::Latest>>;

	fn from_latest(latest: Self::Latest, version: u8) -> Self {
		latest
			.map(|response| Response::from_latest(response, version))
			.map_err(|error| error.map_domain(|domain| Error::from_latest(domain, version)))
	}
}

impl<Item, Error> VersionedOutput for Subscription<Item, CallError<Error>>
where
	Item: FromLatest + 'static,
	Error: FromLatest + 'static,
	Item::Latest: 'static,
	Error::Latest: 'static,
{
	type Latest = Subscription<Item::Latest, CallError<Error::Latest>>;

	fn from_latest(latest: Self::Latest, version: u8) -> Self {
		Self::new(latest.map(move |result| VersionedOutput::from_latest(result, version)))
	}
}

#[cfg(test)]
mod tests;
