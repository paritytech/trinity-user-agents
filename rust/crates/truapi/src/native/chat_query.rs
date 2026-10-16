//! Native observation of the chat queries a host asks a product.

use std::sync::{Arc, Mutex};

use futures::{
	StreamExt,
	future::{AbortHandle, Abortable},
};
use truapi::{CallError, latest};

use crate::{host_core::ChatQuery, subscription::Spawner};

/// The answer a product gave to one chat query, in the kind of the query.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeChatQueryAnswer {
	/// The answer to `chat_messages_query`.
	Messages {
		/// One page of messages.
		page: latest::ProductChatMessagesQueryItem,
	},
	/// The answer to `chat_user_action`.
	UserAction {
		/// The stored message, or an acknowledgement.
		answer: latest::ProductChatUserActionItem,
	},
	/// The answer to `chat_reactions_query`.
	Reactions {
		/// Who reacted.
		reactions: latest::ProductChatReactionsQueryItem,
	},
	/// The answer to `chat_participants_query`.
	Participants {
		/// The count, the page or the participants asked for.
		answer: latest::ProductChatParticipantsQueryItem,
	},
	/// The answer to `chat_message_actions_query`.
	MessageMenu {
		/// The menu of the message.
		menu: latest::ProductChatMessageActionsQueryItem,
	},
	/// The answer to `chat_participant_actions_query`.
	ParticipantMenu {
		/// The menu of the participant.
		menu: latest::ProductChatParticipantActionsQueryItem,
	},
	/// The answer to `chat_search_query`.
	Search {
		/// One page of matches, newest first.
		page: latest::ProductChatSearchQueryItem,
	},
	/// The answer to `chat_open_link`.
	OpenLink {
		/// The room the host shows next.
		answer: latest::ProductChatOpenLinkItem,
	},
}

/// Why a chat query has no answer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeChatQueryRefusal {
	/// The product, or the core for a room that does not declare the handler,
	/// refused a query. `NotAvailable` asks the host to apply its default.
	Query {
		/// The refusal.
		error: latest::ProductChatQueryError,
	},
	/// The product, or the core for a room the host stores, refused a user
	/// action. The host reverts the action and shows `Rejected` reasons.
	UserAction {
		/// The refusal.
		error: latest::ProductChatUserActionError,
	},
	/// The query failed on its way, for example because the worker stopped.
	Failed {
		/// Human-readable reason.
		reason: String,
	},
}

/// Observer implemented by a native host to receive the answer to one chat
/// query. Exactly one method is called once.
#[uniffi::export(callback_interface)]
pub trait NativeChatQueryObserver: Send + Sync {
	/// Deliver the answer of the product.
	fn on_answer(&self, answer: NativeChatQueryAnswer);

	/// Report that the query has no answer, and why.
	fn on_refusal(&self, refusal: NativeChatQueryRefusal);

	/// Report that the product ended the query without an answer. The host
	/// treats the product as unavailable for this query.
	fn on_complete(&self);
}

/// Cancellable native observation of one chat query.
#[derive(uniffi::Object)]
pub struct NativeChatQuery {
	abort: Mutex<Option<AbortHandle>>,
}

#[uniffi::export]
impl NativeChatQuery {
	/// Stop waiting for the answer, and stop the query in the product. The
	/// observer receives nothing after this call.
	pub fn cancel(&self) {
		if let Some(abort) = self.abort.lock().expect("native chat query mutex poisoned").take() {
			abort.abort();
		}
	}
}

impl Drop for NativeChatQuery {
	fn drop(&mut self) {
		self.cancel();
	}
}

/// Deliver the one answer of `query` to `observer`, shaped by `answer` and
/// `refusal`.
pub fn observe_chat_query<Item, Error>(
	mut query: ChatQuery<Item, Error>,
	observer: Arc<dyn NativeChatQueryObserver>,
	spawner: Spawner,
	answer: fn(Item) -> NativeChatQueryAnswer,
	refusal: fn(Error) -> NativeChatQueryRefusal,
) -> Arc<NativeChatQuery>
where
	Item: Send + 'static,
	Error: Send + 'static,
{
	let (abort, registration) = AbortHandle::new_pair();
	(spawner)(Box::pin(async move {
		let _ = Abortable::new(
			async move {
				match query.next().await {
					Some(Ok(item)) => observer.on_answer(answer(item)),
					Some(Err(error)) => observer.on_refusal(match error {
						CallError::Domain(error) => refusal(error),
						CallError::Denied => failed("denied"),
						CallError::Unsupported => failed("unsupported"),
						CallError::Cancelled => failed("cancelled"),
						CallError::MalformedFrame { reason } |
						CallError::HostFailure { reason } => NativeChatQueryRefusal::Failed { reason },
					}),
					None => observer.on_complete(),
				}
			},
			registration,
		)
		.await;
	}));
	Arc::new(NativeChatQuery { abort: Mutex::new(Some(abort)) })
}

fn failed(reason: &str) -> NativeChatQueryRefusal {
	NativeChatQueryRefusal::Failed { reason: reason.to_string() }
}
