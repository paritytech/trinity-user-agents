//! What the CLI does for the stack the core runs: fetch its descriptor and
//! report what changes.

use trinity_stack::{Descriptor, Pointer};
use truapi::{
	latest::GenericError,
	platform::{
		StackFetchRequest, StackFetchResponse, StackHost, StackRefreshOutcome, StackStatus,
	},
};

use super::stack::routes;
use crate::stack_descriptor::{FETCH_TIMEOUT, Served, fetch_if_changed, light_client_chains};

/// The CLI's [`StackHost`] for one stack.
#[derive(Clone)]
pub struct CliStackHost {
	pointer: Pointer,
	on_light_client: bool,
}

impl CliStackHost {
	/// The host of `pointer`'s stack, run on the light client when
	/// `on_light_client` is set.
	pub fn new(pointer: Pointer, on_light_client: bool) -> Self {
		Self { pointer, on_light_client }
	}

	/// Why the CLI cannot run on `descriptor`, or `None` when it can. The core
	/// keeps the copy it holds rather than take one the CLI would fail on.
	fn unrunnable(&self, descriptor: &Descriptor) -> Option<String> {
		if let Err(error) = routes(descriptor) {
			return Some(format!("{error:#}"));
		}
		let missing = light_client_chains(descriptor).find(|chain| chain.light_spec.is_none());
		match missing {
			Some(chain) if self.on_light_client => Some(format!(
				"it names no lightSpec for {}, which the light client needs",
				chain.role
			)),
			_ => None,
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
		let served =
			fetch_if_changed(&request.url, FETCH_TIMEOUT, request.if_none_match.as_deref())
				.await
				.map_err(|error| GenericError::from(format!("{}: {error}", request.url)))?;
		let (bytes, etag) = match served {
			Served::NotModified => return Ok(StackFetchResponse::NotModified),
			Served::Document { body, etag } => (body, etag),
		};
		if let Ok(opened) = trinity_stack::open(&bytes, &self.pointer) &&
			let Some(reason) = self.unrunnable(&opened.descriptor)
		{
			return Err(format!("{} is a copy the CLI cannot run: {reason}", request.url).into());
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
		if let Some(StackRefreshOutcome::Refused { reason }) = status.last_refresh {
			tracing::warn!(
				"the descriptor at {url} is refused, so the CLI keeps its copy: {reason}"
			);
		}
	}
}
