//! Watching the statements a topic filter matches: every matching statement
//! already stored as one batch, then each page of new statements.

use futures::stream::{self, BoxStream, Stream, StreamExt};
use thiserror::Error;
use truapi::latest::RemoteStatementStoreSubscribeRequest;

use super::{NewStatements, StatementStoreParseError};

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
		filter: RemoteStatementStoreSubscribeRequest,
	) -> Result<StatementBatches, StatementWatchError>;
}

/// Joins every page up to and including the first with `remaining == Some(0)`
/// into one batch, then passes each later page on as its own batch. An `Err`
/// passes on as it is.
pub fn backlog_then_live_batches<E: Send + 'static>(
	pages: impl Stream<Item = Result<NewStatements, E>> + Send + 'static,
) -> BoxStream<'static, Result<Vec<Vec<u8>>, E>> {
	let joining = BatchJoin { pages: pages.boxed(), backlog: Some(Vec::new()) };
	stream::unfold(joining, BatchJoin::next_batch).boxed()
}

/// Pages being joined into batches, and the backlog joined so far, `None`
/// once it is complete.
struct BatchJoin<E> {
	pages: BoxStream<'static, Result<NewStatements, E>>,
	backlog: Option<Vec<Vec<u8>>>,
}

/// What adding a page gives: the backlog still being joined, and the batch the
/// page completed, if any.
struct JoinedPage<E> {
	backlog: Option<Vec<Vec<u8>>>,
	batch: Option<Result<Vec<Vec<u8>>, E>>,
}

impl<E> BatchJoin<E> {
	/// The next batch, with the join that yields the ones after it, or `None`
	/// once the pages end.
	async fn next_batch(mut self) -> Option<(Result<Vec<Vec<u8>>, E>, Self)> {
		loop {
			let page = self.pages.next().await?;
			let joined = join_page(self.backlog.take(), page);
			self.backlog = joined.backlog;
			if let Some(batch) = joined.batch {
				return Some((batch, self));
			}
		}
	}
}

/// Adds `page` to `backlog` while it is incomplete, and makes a batch of the
/// backlog once `page` completes it, or of `page` alone after that.
fn join_page<E>(backlog: Option<Vec<Vec<u8>>>, page: Result<NewStatements, E>) -> JoinedPage<E> {
	let page = match page {
		Ok(page) => page,
		Err(error) => return JoinedPage { backlog, batch: Some(Err(error)) },
	};
	let Some(mut backlog) = backlog else {
		return JoinedPage { backlog: None, batch: Some(Ok(page.statements)) };
	};
	backlog.extend(page.statements);
	if page.remaining == Some(0) {
		return JoinedPage { backlog: None, batch: Some(Ok(backlog)) };
	}
	JoinedPage { backlog: Some(backlog), batch: None }
}

#[cfg(test)]
mod tests {
	use futures::executor::block_on_stream;

	use super::*;

	fn statement_page(statements: &[u8], remaining: Option<u64>) -> Result<NewStatements, String> {
		let statements = statements.iter().map(|statement| vec![*statement]).collect();
		Ok(NewStatements { remote_subscription_id: "sub".to_string(), statements, remaining })
	}

	fn batches_of(pages: Vec<Result<NewStatements, String>>) -> Vec<Result<Vec<Vec<u8>>, String>> {
		block_on_stream(backlog_then_live_batches(stream::iter(pages))).collect()
	}

	/// A product reading an inbox gets everything already there in one item,
	/// then each new page as it arrives.
	#[test]
	fn the_backlog_is_one_batch_and_each_later_page_its_own() {
		let pages = vec![
			statement_page(&[1, 2], Some(1)),
			statement_page(&[3], Some(0)),
			statement_page(&[4], None),
			statement_page(&[5], None),
		];

		let expected =
			vec![Ok(vec![vec![1], vec![2], vec![3]]), Ok(vec![vec![4]]), Ok(vec![vec![5]])];
		assert_eq!(batches_of(pages), expected);
	}

	/// An empty topic still reports that the backlog is complete.
	#[test]
	fn an_empty_backlog_is_one_empty_batch() {
		let pages = vec![statement_page(&[], Some(0))];

		assert_eq!(batches_of(pages), vec![Ok(vec![])]);
	}

	/// The node may interleave new statements, which carry no `remaining`,
	/// with the backlog pages.
	#[test]
	fn a_page_without_remaining_before_the_backlog_ends_joins_the_backlog() {
		let pages = vec![
			statement_page(&[1], Some(1)),
			statement_page(&[2], None),
			statement_page(&[3], Some(0)),
		];

		assert_eq!(batches_of(pages), vec![Ok(vec![vec![1], vec![2], vec![3]])]);
	}

	/// A failure must reach the reader, wherever it happens.
	#[test]
	fn errors_pass_on_before_and_after_the_backlog() {
		let failure = "transport".to_string();
		let pages = vec![
			statement_page(&[1], Some(1)),
			Err(failure.clone()),
			statement_page(&[2], Some(0)),
			Err(failure.clone()),
		];

		let expected = vec![Err(failure.clone()), Ok(vec![vec![1], vec![2]]), Err(failure)];
		assert_eq!(batches_of(pages), expected);
	}
}
