//! Runs a product worker compiled to wasm as the selected product.

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use futures::future::BoxFuture;
use truapi::{
	ChatStageRejection, ChatWakeOutcome, ChatWakeReason, HostAdmin, ProductRuntimeError,
	SigningHostRuntime, WasmEnv, WasmWorker,
	latest::{
		HostChatActionSubscribeItem, HostChatRoomEventsSubscribeItem,
		ProductChatMessageActionsQueryItem, ProductChatMessageActionsQueryRequest,
		ProductChatMessagesQueryItem, ProductChatMessagesQueryRequest,
		ProductChatParticipantActionsQueryItem, ProductChatParticipantActionsQueryRequest,
		ProductChatParticipantsQueryItem, ProductChatParticipantsQueryRequest,
		ProductChatReactionsQueryItem, ProductChatReactionsQueryRequest,
		ProductChatSearchQueryItem, ProductChatSearchQueryRequest, ProductChatUserActionError,
		ProductChatUserActionItem, ProductChatUserActionRequest,
	},
	platform::{ProductContext, ProductExecutionKind},
};

use crate::chat_surface::{ChatSurface, ChatWorker, Query};

/// The context a worker for `product_id` runs under.
pub fn context(product_id: &str) -> Result<ProductContext> {
	ProductContext::new_with_execution(product_id.to_string(), ProductExecutionKind::Worker)
		.map_err(|error| anyhow::anyhow!("invalid product id: {error}"))
}

/// Run the worker at `path` against `admin`'s product until its entry point
/// returns, passing each line it logs to `log`.
pub async fn run(
	admin: HostAdmin,
	path: &Path,
	log: impl Fn(&str) + Send + Sync + 'static,
) -> Result<()> {
	let wasm = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
	let env = WasmEnv::for_product(admin.product_runtime().clone());
	WasmWorker::new(env, &wasm)?.with_log(log).run().await?;
	Ok(())
}

/// Run the worker at `path` as the `Worker` of `product_id` on `runtime`, and
/// attach it to `surface` until its entry point returns. The page of the
/// surface shows the rooms of the worker, and what a person does on the page
/// reaches the worker through `chat.actionSubscribe()`.
pub async fn run_on_surface(
	runtime: &SigningHostRuntime,
	surface: &Arc<ChatSurface>,
	product_id: &str,
	path: &Path,
	log: impl Fn(&str) + Send + Sync + 'static,
) -> Result<()> {
	let wasm = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
	let admin = Arc::new(runtime.product_admin(context(product_id)?));
	let env = WasmEnv::for_product(admin.product_runtime().clone());
	let worker = WasmWorker::new(env, &wasm)?.with_log(log);
	let chat: Arc<dyn ChatWorker> = Arc::new(WasmChat { admin });
	let _attachment = surface.attach_worker(&chat);
	worker.run().await?;
	Ok(())
}

/// The chat side of a wasm worker. The surface sends events to the product
/// runtime of the worker. A wasm worker cannot answer a call from the host
/// yet, so each query that the host starts is unsupported. Thus a wasm worker
/// can keep only rooms that the host stores.
struct WasmChat {
	admin: Arc<HostAdmin>,
}

impl WasmChat {
	fn unsupported<T>() -> Result<T, ProductRuntimeError> {
		Err(ProductRuntimeError::Unsupported)
	}
}

impl ChatWorker for WasmChat {
	fn publish_chat_event(
		&self,
		item: HostChatActionSubscribeItem,
	) -> Result<(), ProductRuntimeError> {
		self.admin.product_runtime().publish_chat_event(item)
	}

	fn messages_query(
		&self,
		_request: ProductChatMessagesQueryRequest,
	) -> Result<Query<ProductChatMessagesQueryItem>, ProductRuntimeError> {
		Self::unsupported()
	}

	fn user_action(
		&self,
		_request: ProductChatUserActionRequest,
	) -> Result<Query<ProductChatUserActionItem, ProductChatUserActionError>, ProductRuntimeError>
	{
		Self::unsupported()
	}

	fn reactions_query(
		&self,
		_request: ProductChatReactionsQueryRequest,
	) -> Result<Query<ProductChatReactionsQueryItem>, ProductRuntimeError> {
		Self::unsupported()
	}

	fn participants_query(
		&self,
		_request: ProductChatParticipantsQueryRequest,
	) -> Result<Query<ProductChatParticipantsQueryItem>, ProductRuntimeError> {
		Self::unsupported()
	}

	fn message_actions_query(
		&self,
		_request: ProductChatMessageActionsQueryRequest,
	) -> Result<Query<ProductChatMessageActionsQueryItem>, ProductRuntimeError> {
		Self::unsupported()
	}

	fn participant_actions_query(
		&self,
		_request: ProductChatParticipantActionsQueryRequest,
	) -> Result<Query<ProductChatParticipantActionsQueryItem>, ProductRuntimeError> {
		Self::unsupported()
	}

	fn publish_chat_room_event(
		&self,
		item: HostChatRoomEventsSubscribeItem,
	) -> Result<(), ProductRuntimeError> {
		let runtime = self.admin.product_runtime();
		runtime.native_chat_platform()?;
		runtime.publish_chat_room_event(item)
	}

	fn wake_product(&self, reason: ChatWakeReason) -> Result<ChatWakeOutcome, ProductRuntimeError> {
		self.admin.product_runtime().wake(reason)
	}

	fn search_query(
		&self,
		_request: ProductChatSearchQueryRequest,
	) -> Result<Query<ProductChatSearchQueryItem>, ProductRuntimeError> {
		Self::unsupported()
	}

	fn stage_chat_attachment(
		&self,
		room_id: String,
		mime_type: String,
		bytes: Vec<u8>,
	) -> BoxFuture<'static, Result<Vec<u8>, ChatStageRejection>> {
		let admin = Arc::clone(&self.admin);
		Box::pin(async move {
			admin.product_runtime().stage_chat_attachment(&room_id, &mime_type, bytes).await
		})
	}
}
