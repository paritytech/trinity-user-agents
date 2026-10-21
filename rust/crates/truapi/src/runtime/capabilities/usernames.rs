//! Product-facing username directory capability adapter.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::Usernames,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		usernames::{
			HostUsernamesSearchError, HostUsernamesSearchRequest, HostUsernamesSearchResponse,
		},
	},
};

use super::from_latest_error;
use crate::runtime::{ProductRuntimeHost, until_cancelled};

/// The most usernames that a search gives.
pub const USERNAMES_MAX_RESULTS: usize = 20;

/// The shortest prefix that a search takes.
const PREFIX_MIN_CHARS: usize = 2;

/// The longest username.
const USERNAME_MAX_CHARS: usize = 64;

/// The reason `text` is no username, or the start of one, or `None` when it
/// is: `min` to 64 letters, digits, `.`, `-` or `_`.
fn invalid_name(text: &str, min: usize) -> Option<String> {
	if text.len() < min {
		return Some(format!("the text has fewer than {min} characters"));
	}
	if text.len() > USERNAME_MAX_CHARS {
		return Some(format!("the text is longer than {USERNAME_MAX_CHARS} characters"));
	}
	(!text
		.bytes()
		.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')))
	.then(|| "the text has a character that no username has".to_string())
}

#[truapi::async_trait]
impl Usernames for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "usernames.search"))]
	async fn search(
		&self,
		cx: &CallContext,
		request: HostUsernamesSearchRequest,
	) -> Result<HostUsernamesSearchResponse, CallError<HostUsernamesSearchError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform =
				self.connection.services.usernames_platform().ok_or(CallError::Unsupported)?;
			let prefix = request.prefix.trim().trim_start_matches('@').to_ascii_lowercase();
			if let Some(reason) = invalid_name(&prefix, PREFIX_MIN_CHARS) {
				return Err(CallError::Domain(latest::HostUsernamesSearchError::InvalidRequest {
					reason,
				}));
			}
			let unknown = |reason: String| {
				CallError::Domain(latest::HostUsernamesSearchError::Unknown { reason })
			};
			let mut usernames =
				until_cancelled(cx, platform.search_usernames(&self.connection.product, &prefix))
					.await
					.map_err(|error| unknown(error.to_string()))?
					.map_err(|error| unknown(error.reason))?;
			// The host is not trusted with the limits: only usernames, at most
			// the maximum, with no repeats.
			usernames.retain(|name| invalid_name(name, 1).is_none());
			usernames.dedup();
			usernames.truncate(USERNAMES_MAX_RESULTS);
			Ok(latest::HostUsernamesSearchResponse { usernames })
		}
		.await;
		result
			.map(|response| HostUsernamesSearchResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_prefix_and_a_username_have_the_characters_of_a_username() {
		assert_eq!(invalid_name("al", PREFIX_MIN_CHARS), None);
		assert!(invalid_name("a", PREFIX_MIN_CHARS).is_some());
		assert_eq!(invalid_name("alice.42", 1), None);
		assert!(invalid_name("alice 42", 1).is_some());
		assert!(invalid_name(&"a".repeat(USERNAME_MAX_CHARS + 1), 1).is_some());
	}
}
