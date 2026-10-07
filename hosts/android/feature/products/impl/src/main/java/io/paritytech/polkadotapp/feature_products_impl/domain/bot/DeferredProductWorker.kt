package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
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
    private val delegate = MutableStateFlow<ProductWorker?>(null)

    fun attach(worker: ProductWorker?) {
        delegate.value = worker
    }

    override suspend fun onUserMessage(text: String): Result<Unit> {
        return delegate.filterNotNull().first().onUserMessage(text)
    }

    override fun renderMessage(
        chatId: ChatId,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>> = flow {
        emitAll(delegate.filterNotNull().first().renderMessage(chatId, messageId, messageType, messageData))
    }

    // UI events only originate from already-rendered widgets, so the worker is attached by then.
    override fun dispatchEvent(event: JsUiEvent) {
        delegate.value?.dispatchEvent(event)
    }
}
