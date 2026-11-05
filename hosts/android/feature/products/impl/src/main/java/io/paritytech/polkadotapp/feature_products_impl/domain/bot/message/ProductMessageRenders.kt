package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import timber.log.Timber
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

/**
 * The renders of one product's chat messages, kept until [close]: a chat opened again shows each
 * message's last tree at once instead of waiting for the worker to draw it anew. A message's
 * render stream is open only while a cell observes its state, and for [streamKeepAlive] after,
 * so a long chat does not hold one live render per message it ever showed. A message that has no
 * tree after [firstTreeWindow] reads as failed, and a failed message is asked for again the next
 * time it is shown.
 */
class ProductMessageRenders(
    private val product: Product,
    private val worker: ProductWorker,
    dispatcher: CoroutineDispatcher,
    private val renderObservers: Set<ProductMessageRenderObserver>,
    private val firstTreeWindow: Duration = FIRST_TREE_WINDOW,
    private val streamKeepAlive: Duration = STREAM_KEEP_ALIVE,
) {
    private class Render(val state: MutableStateFlow<LoadingState<JsWidget>>, val job: Job)

    private class NoTreeDrawn(window: Duration) : Exception("no tree was drawn within $window")

    private class Closed : Exception("the product's message renders were closed")

    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val renders = mutableMapOf<ChatMessageId, Render>()

    fun render(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        content: ProductsMessageContent,
    ): StateFlow<LoadingState<JsWidget>> =
        synchronized(renders) {
            val kept = renders[messageId]?.takeUnless { it.state.value is LoadingState.Error }
            kept ?: open(roomId, messageId, content).also { renders[messageId] = it }
        }.state.asStateFlow()

    fun dispatchEvent(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        content: ProductsMessageContent,
        actionId: String,
        eventType: JsUiEvent.Type,
    ) {
        worker.dispatchEvent(
            JsUiEvent(
                messageId = messageId,
                messageType = content.messageType,
                actionId = actionId,
                eventType = eventType,
                roomId = roomId,
            )
        )
    }

    /** Stops every render; a cell still loading reads as failed, one that drew keeps its tree. */
    fun close() {
        scope.cancel()
        synchronized(renders) {
            renders.values.forEach { it.state.compareAndSet(LoadingState.Loading, LoadingState.Error(Closed())) }
        }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun open(roomId: ProductChatIdParameter?, messageId: ChatMessageId, content: ProductsMessageContent): Render {
        renders[messageId]?.job?.cancel()
        val state = MutableStateFlow<LoadingState<JsWidget>>(
            if (scope.isActive) LoadingState.Loading else LoadingState.Error(Closed())
        )
        val job = scope.launch {
            state.subscriptionCount
                .map { subscribers -> subscribers > 0 }
                .distinctUntilChanged()
                .transformLatest { observed ->
                    if (!observed) delay(streamKeepAlive)
                    emit(observed)
                }
                .distinctUntilChanged()
                .collectLatest { observed -> if (observed) draw(roomId, messageId, content, state) }
        }
        return Render(state, job)
    }

    private suspend fun draw(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        content: ProductsMessageContent,
        state: MutableStateFlow<LoadingState<JsWidget>>,
    ) = coroutineScope {
        val deadline = launch {
            delay(firstTreeWindow)
            state.compareAndSet(LoadingState.Loading, LoadingState.Error(NoTreeDrawn(firstTreeWindow)))
        }
        try {
            worker.renderMessage(roomId, messageId, content.messageType, content.data).collect { result ->
                deadline.cancel()
                result
                    .logFailure("Error receiving render update for message $messageId in ${product.id}")
                    .onSuccess { widget ->
                        state.value = LoadingState.Loaded(widget)
                        renderObservers.forEach { it.onMessageRendered(product.id) }
                    }
                    .onFailure { error -> state.value = LoadingState.Error(error) }
            }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Timber.e(error, "Render of message %s in %s threw", messageId, product.id.value)
            deadline.cancel()
            state.value = LoadingState.Error(error)
        }
    }

    private companion object {
        // Covers a worker booting from cold; three five-second streams on iOS wait about as long.
        val FIRST_TREE_WINDOW = 15.seconds

        // Long enough that leaving a chat and coming back reopens nothing.
        val STREAM_KEEP_ALIVE = 1.minutes
    }
}
