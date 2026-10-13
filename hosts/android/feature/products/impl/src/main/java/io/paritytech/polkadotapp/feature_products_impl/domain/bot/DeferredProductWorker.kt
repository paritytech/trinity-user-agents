package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow

/**
 * A stable [ProductWorker] the message renderer can hold from construction. It resolves to the
 * shared worker once chat acquires it for driving, so a custom message queued before boot completes
 * still renders against the real instance.
 */
class DeferredProductWorker : ProductWorker {
    private val delegate = MutableStateFlow<Result<ProductWorker>?>(null)

    fun attach(worker: ProductWorker) {
        delegate.value = Result.success(worker)
    }

    fun fail(cause: Throwable) {
        delegate.value = Result.failure(cause)
    }

    private suspend fun awaitWorker(): Result<ProductWorker> = delegate.filterNotNull().first()

    override suspend fun onUserMessage(roomId: ProductChatIdParameter?, text: String): Result<Unit> =
        awaitWorker().fold(
            onSuccess = { it.onUserMessage(roomId, text) },
            onFailure = { Result.failure(it) },
        )

    override fun renderMessage(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>> = flow {
        awaitWorker().fold(
            onSuccess = { emitAll(it.renderMessage(roomId, messageId, messageType, messageData)) },
            onFailure = { emit(Result.failure(it)) },
        )
    }

    // UI events only originate from already-rendered widgets, so the worker is attached by then.
    override fun dispatchEvent(event: JsUiEvent) {
        delegate.value?.getOrNull()?.dispatchEvent(event)
    }
}
