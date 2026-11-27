//! What the CLI does for the stack the core runs: fetch its descriptor and
//! report what changes.

use std::time::Duration;

use trinity_stack::Pointer;
use truapi::{
	latest::GenericError,
	platform::{
		StackFetchRequest, StackFetchResponse, StackHost, StackRefreshOutcome, StackStatus,
	},
};

use super::stack::unrunnable;
use crate::stack_descriptor::fetch_if_changed;

/// The CLI's [`StackHost`] for one stack.
#[derive(Clone)]
pub struct CliStackHost {
	pointer: Pointer,
	/// Whether a copy must name a light spec for each chain the light client
	/// runs, as it must when this run is on the light client.
	needs_light_specs: bool,
	/// How long a fetch waits for the server.
	timeout: Duration,
}

impl CliStackHost {
	/// The host of `pointer`'s stack, run on the light client when
	/// `on_light_client` is set, whose fetches wait at most `timeout` for the
	/// server.
	pub fn new(pointer: Pointer, on_light_client: bool, timeout: Duration) -> Self {
		Self { pointer, needs_light_specs: on_light_client, timeout }
	}

	/// Warns when the last fetch brought a copy the core refused, such as one
	/// on another TLD. A command that fails after a fetch it waited on calls
	/// this; one that runs is told the same by [`StackHost::stack_changed`]
	/// once its background refresh starts.
	pub fn warn_if_refused(&self, status: &StackStatus) {
		if let Some(StackRefreshOutcome::Refused { reason }) = &status.last_refresh {
			let url = self.pointer.url();
			tracing::warn!(
				"the descriptor at {url} is refused, so the CLI keeps its copy: {reason}"
			);
		}
	}
}

#[async_trait::async_trait]
impl StackHost for CliStackHost {
	/// Refuses a copy the CLI cannot run as a failed fetch, so the core keeps
	/// the one it holds. A copy that does not open under the pointer is passed
	/// on for the core to refuse with its own reason.
	async fn fetch_stack_descriptor(
		&self,
		request: StackFetchRequest,
	) -> Result<StackFetchResponse, GenericError> {
		let StackFetchRequest { url, if_none_match, max_bytes } = request;
		let max_bytes = usize::try_from(max_bytes).unwrap_or(usize::MAX);
		let fetched = fetch_if_changed(&url, self.timeout, if_none_match.as_deref(), max_bytes)
			.await
			.map_err(|error| GenericError::from(format!("{url}: {error}")))?;
		let (bytes, etag) = match fetched {
			StackFetchResponse::Document { bytes, etag } => (bytes, etag),
			StackFetchResponse::NotModified => return Ok(StackFetchResponse::NotModified),
		};
		if let Ok(opened) = trinity_stack::open(&bytes, &self.pointer) &&
			let Some(reason) = unrunnable(&opened.descriptor, self.needs_light_specs)
		{
			let refused = format!("{url} is a copy the CLI cannot run: {reason}");
			tracing::warn!("{refused}; the CLI keeps the copy it holds");
			return Err(refused.into());
		}
		Ok(StackFetchResponse::Document { bytes, etag })
	}

	fn stack_changed(&self, status: StackStatus) {
		let url = self.pointer.url();
		if status.restart_required {
			tracing::warn!(
				"a newer descriptor of the stack at {url} changes its chains; restart truapi-host \
				 to run on it"
			);
		}
		if status.expired {
			tracing::warn!("the stack at {url} is past expiresAt; it has shut down");
		}
		self.warn_if_refused(&status);
	}
}

#[cfg(test)]
mod tests;
