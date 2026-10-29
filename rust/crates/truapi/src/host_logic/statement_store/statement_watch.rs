//! Watching the statements a topic filter matches: every matching statement
//! already stored as one batch, then each page of new statements.

use futures::stream::BoxStream;
use thiserror::Error;
use truapi::latest::RemoteStatementStoreSubscribeRequest;

use super::StatementStoreParseError;

/// Which statements a watch matches, by their topics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopicFilter {
	/// Statements that carry every one of these topics.
	MatchAll(Vec<[u8; 32]>),
	/// Statements that carry at least one of these topics.
	MatchAny(Vec<[u8; 32]>),
}

impl From<RemoteStatementStoreSubscribeRequest> for TopicFilter {
	fn from(request: RemoteStatementStoreSubscribeRequest) -> Self {
		match request {
			RemoteStatementStoreSubscribeRequest::MatchAll(topics) => Self::MatchAll(topics),
			RemoteStatementStoreSubscribeRequest::MatchAny(topics) => Self::MatchAny(topics),
		}
	}
}

/// SCALE-encoded signed statements, batch by batch.
pub type StatementBatches = BoxStream<'static, Result<Vec<Vec<u8>>, StatementWatchError>>;

/// Why a statement watch did not start, or stopped.
#[derive(Debug, Error)]
pub enum StatementWatchError {
	/// The watch did not start.
	#[error("the statement watch did not start: {0}")]
	NotStarted(String),
	/// The subscription failed after it started.
	#[error("the statement subscription failed: {0}")]
	Subscription(String),
	/// The node sent a page that does not parse.
	#[error("the statement store sent a malformed page: {0}")]
	MalformedPage(StatementStoreParseError),
}

/// The People chain's statement store, as host code reads it.
#[async_trait::async_trait]
pub trait StatementStoreApi: Send + Sync {
	/// Starts watching the statements `filter` matches; `Err` means the watch
	/// did not start. The first item holds every matching statement already
	/// stored, possibly none. Each later item holds one page of new
	/// statements. The stream ends after its first `Err` item.
	async fn start_statement_watch(
		&self,
		filter: TopicFilter,
	) -> Result<StatementBatches, StatementWatchError>;
}
