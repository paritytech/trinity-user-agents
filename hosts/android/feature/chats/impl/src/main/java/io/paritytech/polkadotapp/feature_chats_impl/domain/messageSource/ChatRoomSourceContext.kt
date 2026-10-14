package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isExtensionChat
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomLifecycle
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.NoChatRoomLifecycle
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
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import timber.log.Timber
import javax.inject.Inject

/** Makes the source of a room whose messages the product keeps. */
fun interface ProductMessageSourceFactory {
    fun create(chatId: ChatId, policy: ChatRoomPolicy, querying: ProductChatQuerying?): ChatMessageSource
}

/** What the source context of one room needs from the engine and the policy. */
interface ChatRoomServices {
    suspend fun policy(chatId: ChatId): ChatRoomPolicy?

    suspend fun lifecycle(chatId: ChatId): ChatRoomLifecycle

    suspend fun querying(chatId: ChatId): ProductChatQuerying?

    fun hostStoreSource(chatId: ChatId): ChatMessageSource
}

class EngineChatRoomServices @Inject constructor(
    private val chatEngine: ChatEngine,
    private val roomPolicyProvider: ChatRoomPolicyProvider,
) : ChatRoomServices {
    override suspend fun policy(chatId: ChatId): ChatRoomPolicy? = roomPolicyProvider.observePolicy(chatId).first()

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
 * Picks the message source of one room from its policy, once, and holds the room open while the room screen
 * shows it (02-DESIGN §5.3, §21 item 5). The open and close calls run in the order of the requests, so a
 * close never passes an open.
 */
class ChatRoomSourceContext(
    private val chatId: ChatId,
    private val services: ChatRoomServices,
    private val productSourceFactory: ProductMessageSourceFactory,
    dispatcher: CoroutineDispatcher,
) {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val resolutionLock = Mutex()
    private var resolution: Deferred<ChatMessageSource>? = null
    private var productSource: ChatMessageSource? = null

    private val roomWork = Channel<suspend () -> Unit>(Channel.UNLIMITED)
    private var openLifecycle: ChatRoomLifecycle? = null

    init {
        scope.launch {
            for (work in roomWork) work()
        }
    }

    suspend fun source(): ChatMessageSource {
        val pending = resolutionLock.withLock {
            resolution ?: scope.async { resolve() }.also { resolution = it }
        }
        return pending.await()
    }

    /** Holds the product of the room while the room screen shows it. A person chat holds nothing. */
    fun open() = enqueue { openRoom() }

    fun close() = enqueue { closeRoom() }

    /** Opens the room again when the last open failed, then asks for the newest page. */
    fun retry() = enqueue {
        openRoom()
        source().reload()
    }

    /** Closes the room when it is open, then stops the source. Call it once, when the room screen goes away. */
    fun release() {
        enqueue {
            closeRoom()
            productSource?.close()
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
            }
    }

    private suspend fun closeRoom() {
        val lifecycle = openLifecycle ?: return
        openLifecycle = null
        lifecycle.close(chatId)
    }

    private suspend fun resolve(): ChatMessageSource {
        val policy = if (chatId.isExtensionChat()) services.policy(chatId) else null
        if (policy?.messageStore != ChatRoomPolicy.MessageStore.PRODUCT) return services.hostStoreSource(chatId)

        return productSourceFactory.create(chatId, policy, services.querying(chatId)).also { productSource = it }
    }
}
