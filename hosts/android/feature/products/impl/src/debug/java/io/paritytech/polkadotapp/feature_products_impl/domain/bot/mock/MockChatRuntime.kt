package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductChatHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductChatStoredState
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.InAppProductRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.job
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import timber.log.Timber
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessageBody
import java.util.concurrent.atomic.AtomicReference
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Serves [MockChatProduct] in place of a worker. Each boot gets a [MockChatWorker] that keeps the rooms in memory, and
 * the room events of the additions, which do not pass the worker, go to the worker of the last boot.
 */
@Singleton
class MockChatRuntime @Inject constructor(
    private val storedState: ProductChatStoredState,
    private val mediaFactory: MockChatMediaFactory,
) : InAppProductRuntime {
    override val productId: ProductId = MockChatProduct.productId

    private val current = AtomicReference<MockChatWorker?>(null)

    override fun boot(botApi: BindableProductsBotApi, scope: CoroutineScope): ProductWorker {
        // The bridge maps the wire values of the showcase as it maps those of a product worker.
        val host = MockChatHost(ProductChatHostBridge(productId, botApi, storedState))
        val worker = MockChatWorker(productId, host, botApi, mediaFactory, scope)
        current.set(worker)
        scope.coroutineContext.job.invokeOnCompletion { current.compareAndSet(worker, null) }
        return worker
    }

    override suspend fun publishRoomEvent(room: ProductChatIdParameter, event: ChatRoomEventModel): Result<Unit> {
        val worker = current.get() ?: return Result.failure(IllegalStateException("the chat showcase does not run"))
        worker.receive(room, event)
        return Result.success(Unit)
    }
}

/** The calls of the mock product to the host. A call that fails is logged, so a showcase that cannot set one part still shows the others. */
class MockChatHost(private val bridge: ProductChatHostBridge) {
    suspend fun perform(what: String, call: suspend ProductChatHostBridge.() -> Unit) {
        runCancellableCatching { bridge.call() }.onFailure { Timber.w(it, "Chat showcase could not %s", what) }
    }
}

/** The messages of `showcase`, the room that the mock product keeps, and what the product shows beside them. */
class MockChatState(val media: MockChatMedia, now: ULong) {
    private val mutex = Mutex()
    private val conversation = MockChatConversation(MockChatShowcaseMessages.showcase(media, now))

    @Volatile var footer = MockChatShowcase.Footer.INPUT

    @Volatile var threadReplies = MockChatShowcaseMessages.thread(now).size

    suspend fun <T> read(block: (MockChatConversation) -> T): T = mutex.withLock { block(conversation) }

    suspend fun apply(update: ChatFeedUpdate) {
        read { conversation -> applyTo(conversation, update) }
    }

    private fun applyTo(conversation: MockChatConversation, update: ChatFeedUpdate) {
        when (update) {
            is ChatFeedUpdate.New -> conversation.append(update.v1)
            is ChatFeedUpdate.Edited -> conversation.edit(update.messageId, update.body, update.editedAt)
            is ChatFeedUpdate.Deleted -> conversation.remove(update.messageId)
            is ChatFeedUpdate.ReactionPlaced -> conversation.react(update.messageId, update.emoji, placed = true, byUser = update.participantId == MockChatShowcase.Participant.USER)
            is ChatFeedUpdate.ReactionRemoved -> conversation.react(update.messageId, update.emoji, placed = false, byUser = update.participantId == MockChatShowcase.Participant.USER)
            is ChatFeedUpdate.StatusChanged -> conversation.setStatus(update.messageId, update.status)
        }
    }
}

fun ChatMessageBody.hasMedia(): Boolean = this is ChatMessageBody.RichText && media.isNotEmpty()
