package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isExtensionChat
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomLifecycle
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

/** What the source context of one room needs from the engine. */
interface ChatRoomServices {
    suspend fun lifecycle(chatId: ChatId): ChatRoomLifecycle

    fun hostStoreSource(chatId: ChatId): ChatMessageSource
}

class EngineChatRoomServices @Inject constructor(
    private val chatEngine: ChatEngine,
) : ChatRoomServices {
    override suspend fun lifecycle(chatId: ChatId): ChatRoomLifecycle = chatEngine.roomLifecycle(chatId)

    override fun hostStoreSource(chatId: ChatId): ChatMessageSource = HostStoreMessageSource(chatId, chatEngine)
}

class ChatRoomSourceContextFactory @Inject constructor(
    private val services: EngineChatRoomServices,
    private val dispatchers: CoroutineDispatchers,
) {
    fun create(chatId: ChatId) = ChatRoomSourceContext(chatId, services, dispatchers.computation)
}

/**
 * Gives the message source of one room, and holds the room open while the room screen shows it. The open and close
 * calls run in the order of the requests, so a close never passes an open.
 */
class ChatRoomSourceContext(
    private val chatId: ChatId,
    private val services: ChatRoomServices,
    dispatcher: CoroutineDispatcher,
) {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val source: ChatMessageSource by lazy { services.hostStoreSource(chatId) }

    private val roomWork = Channel<suspend () -> Unit>(Channel.UNLIMITED)
    private var openLifecycle: ChatRoomLifecycle? = null
    private var wantsOpen = false
    private var openFailures = 0

    init {
        scope.launch {
            for (work in roomWork) work()
        }
    }

    fun source(): ChatMessageSource = source

    /** The source of the room. */
    fun observeSource(): Flow<ChatMessageSource> = flow { emit(source()) }

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
            source.close()
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

    private companion object {
        val OPEN_RETRY_DELAY = 2.seconds
        val MAX_OPEN_RETRY_DELAY = 60.seconds
        const val MAX_OPEN_BACKOFF_SHIFT = 5
    }
}
