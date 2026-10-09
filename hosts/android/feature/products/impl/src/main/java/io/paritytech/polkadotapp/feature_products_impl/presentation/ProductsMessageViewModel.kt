package io.paritytech.polkadotapp.feature_products_impl.presentation

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductMessageRenderObserver
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

/**
 * ViewModel for rendering a single Products message.
 *
 * Takes the message content (scriptId + data bytes) and asynchronously
 * invokes the script executor to produce a widget tree.
 */
@HiltViewModel(assistedFactory = ProductsMessageViewModel.Factory::class)
class ProductsMessageViewModel @AssistedInject constructor(
    @Assisted private val content: ProductsMessageContent,
    @Assisted private val messageId: ChatMessageId,
    @Assisted private val worker: ProductWorker,
    @Assisted private val product: Product,
    // Dagger cannot take a value class here: Kotlin mangles the factory method name.
    @Assisted("roomId") private val roomId: String?,
    private val renderObservers: Set<@JvmSuppressWildcards ProductMessageRenderObserver>,
) : BaseViewModel() {
    private val room = roomId?.let(::ProductChatIdParameter)

    private val _state = MutableStateFlow<LoadingState<JsWidget>>(LoadingState.Loading)
    val state: StateFlow<LoadingState<JsWidget>> = _state.asStateFlow()

    init {
        loadWidget()
    }

    fun handleUiEvent(actionId: String, eventType: JsUiEvent.Type) {
        val event = JsUiEvent(
            messageId = messageId,
            messageType = content.messageType,
            actionId = actionId,
            eventType = eventType,
            roomId = room,
        )
        worker.dispatchEvent(event)
    }

    private fun loadWidget() {
        worker.renderMessage(room, messageId, content.messageType, content.data)
            .onEach { result -> handleRenderUpdate(result) }
            .launchIn(this)
    }

    private fun handleRenderUpdate(result: Result<JsWidget>) {
        result
            .logFailure("Error receiving render update  for message $messageId in ${product.id}")
            .onSuccess { widget ->
                _state.value = LoadingState.Loaded(widget)
                renderObservers.forEach { it.onMessageRendered(product.id) }
            }
            .onFailure { error ->
                _state.value = LoadingState.Error(error)
            }
    }

    @AssistedFactory
    interface Factory {
        fun create(
            content: ProductsMessageContent,
            messageId: ChatMessageId,
            product: Product,
            worker: ProductWorker,
            @Assisted("roomId") roomId: String?,
        ): ProductsMessageViewModel
    }
}
