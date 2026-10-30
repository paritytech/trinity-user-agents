package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isExtensionChat
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomLifecycle
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

/** Makes the source of a room whose messages the product keeps. */
fun interface ProductMessageSourceFactory {
    fun create(chatId: ChatId, policy: ChatRoomPolicy, querying: ProductChatQuerying?): ChatMessageSource
}

/** What the source context of one room needs from the engine and the policy. */
interface ChatRoomServices {
    suspend fun policy(chatId: ChatId): ChatRoomPolicy?

    fun observePolicy(chatId: ChatId): Flow<ChatRoomPolicy?>

    suspend fun lifecycle(chatId: ChatId): ChatRoomLifecycle

    suspend fun querying(chatId: ChatId): ProductChatQuerying?

    fun hostStoreSource(chatId: ChatId): ChatMessageSource
}

class EngineChatRoomServices @Inject constructor(
    private val chatEngine: ChatEngine,
    private val roomPolicyProvider: ChatRoomPolicyProvider,
) : ChatRoomServices {
    override suspend fun policy(chatId: ChatId): ChatRoomPolicy? = roomPolicyProvider.observePolicy(chatId).first()

    override fun observePolicy(chatId: ChatId): Flow<ChatRoomPolicy?> = roomPolicyProvider.observePolicy(chatId)

    override suspend fun lifecycle(chatId: ChatId): ChatRoomLifecycle = chatEngine.roomLifecycle(chatId)

    override suspend fun querying(chatId: ChatId): ProductChatQuerying? = chatEngine.roomQuerying(chatId)

    override fun hostStoreSource(chatId: ChatId): ChatMessageSource = HostStoreMessageSource(chatId, chatEngine)
}

class ChatRoomSourceContextFactory @Inject constructor(
    private val services: EngineChatRoomServices,
    private val productSourceFactory: ProductMessageSourceFactory,
    private val dispatchers: CoroutineDispatchers,
) {
    fun create(chatId: ChatId) = ChatRoomSourceContext(chatId, services, productSourceFactory, dispatchers.computation)
}

/**
 * Picks the message source of one room from its policy, and holds the room open while the room screen shows it.
 * When the product moves the messages of an open room to the other store, the room gets the source of that store.
 * The open and close calls run in the order of the requests, so a close never passes an open.
 */
class ChatRoomSourceContext(
    private val chatId: ChatId,
    private val services: ChatRoomServices,
    private val productSourceFactory: ProductMessageSourceFactory,
    dispatcher: CoroutineDispatcher,
) {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val resolutionLock = Mutex()
    private var resolution: Deferred<ResolvedSource>? = null

    private val roomWork = Channel<suspend () -> Unit>(Channel.UNLIMITED)
    private var openLifecycle: ChatRoomLifecycle? = null
    private var wantsOpen = false
    private var openFailures = 0

    init {
        scope.launch {
            for (work in roomWork) work()
        }
    }

    suspend fun source(): ChatMessageSource {
        val pending = resolutionLock.withLock {
            resolution ?: scope.async { resolve(services.policy(chatId)) }.also { resolution = it }
        }
        return pending.await().source
    }

    /** The source of the room, and a new one each time the product moves the messages of the room to the other store. */
    fun observeSource(): Flow<ChatMessageSource> = services.observePolicy(chatId)
        .distinctUntilChangedBy { storeOf(it) }
        .map { policy -> sourceFor(policy) }

    /** The source of the room when the product keeps its messages, or null when the host database keeps them. */
    suspend fun productSource(): ChatMessageSource? {
        source()
        return resolution?.await()?.takeIf { it.store == ChatRoomPolicy.MessageStore.PRODUCT }?.source
    }

    /**
     * Holds the product of the room while the room screen shows it. A person chat holds nothing. A room that does not
     * open opens again after a delay, with its newest page, as long as the screen shows it.
     */
    fun open() = enqueue {
        wantsOpen = true
        openFailures = 0
        openRoom()
    }

    fun close() = enqueue {
        wantsOpen = false
        closeRoom()
    }

    /** Closes the room when it is open, then stops the source. Call it once, when the room screen goes away. */
    fun release() {
        enqueue {
            closeRoom()
            productSource()?.close()
            scope.cancel()
        }
        roomWork.close()
    }

    private fun enqueue(work: suspend () -> Unit) {
        roomWork.trySend(work)
    }

    private suspend fun openRoom() {
        if (openLifecycle != null || !chatId.isExtensionChat()) return

        val lifecycle = services.lifecycle(chatId)
        lifecycle.open(chatId)
            .onSuccess { openLifecycle = lifecycle }
            .onFailure {
                Timber.w(it, "The product room did not open")
                source().markUnavailable()
                openAgainLater()
            }
    }

    private fun openAgainLater() {
        val wait = (OPEN_RETRY_DELAY * (1 shl openFailures.coerceAtMost(MAX_OPEN_BACKOFF_SHIFT))).coerceAtMost(MAX_OPEN_RETRY_DELAY)
        openFailures++
        scope.launch {
            delay(wait)
            enqueue {
                if (!wantsOpen || openLifecycle != null) return@enqueue
                openRoom()
                if (openLifecycle != null) source().reload()
            }
        }
    }

    private suspend fun closeRoom() {
        val lifecycle = openLifecycle ?: return
        openLifecycle = null
        lifecycle.close(chatId)
    }

    /** The source for [policy]. A change of store replaces the source, and the source of the product stops. */
    private suspend fun sourceFor(policy: ChatRoomPolicy?): ChatMessageSource {
        val store = storeOf(policy)
        val (pending, replaced) = resolutionLock.withLock {
            val current = resolution
            if (current != null && current.await().store == store) return@withLock current to null

            scope.async { resolve(policy) }.also { resolution = it } to current
        }
        replaced?.await()?.takeIf { it.store == ChatRoomPolicy.MessageStore.PRODUCT }?.source?.close()
        return pending.await().source
    }

    private suspend fun resolve(policy: ChatRoomPolicy?): ResolvedSource = when (storeOf(policy)) {
        ChatRoomPolicy.MessageStore.HOST -> ResolvedSource(ChatRoomPolicy.MessageStore.HOST, services.hostStoreSource(chatId))
        ChatRoomPolicy.MessageStore.PRODUCT -> ResolvedSource(
            ChatRoomPolicy.MessageStore.PRODUCT,
            productSourceFactory.create(chatId, requireNotNull(policy), services.querying(chatId)),
        )
    }

    private fun storeOf(policy: ChatRoomPolicy?): ChatRoomPolicy.MessageStore =
        if (chatId.isExtensionChat() && policy?.messageStore == ChatRoomPolicy.MessageStore.PRODUCT) {
            ChatRoomPolicy.MessageStore.PRODUCT
        } else {
            ChatRoomPolicy.MessageStore.HOST
        }

    private class ResolvedSource(val store: ChatRoomPolicy.MessageStore, val source: ChatMessageSource)

    private companion object {
        val OPEN_RETRY_DELAY = 2.seconds
        val MAX_OPEN_RETRY_DELAY = 60.seconds
        const val MAX_OPEN_BACKOFF_SHIFT = 5
    }
}
