package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import kotlinx.coroutines.launch
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Rings for the incoming call of a person chat of the chat product as the native chat rings: the full-screen call
 * screen with the ringtone, until the product clears the call. A call rings once, as the native chat rings an offer
 * once: from its push, or when its room sets it ringing.
 *
 * The rung calls are kept in memory, as the calls of the rooms are: the native table of processed messages takes only
 * the ids of stored native messages, and a product call has none.
 */
@Singleton
class ProductRoomRinger @Inject constructor(
    private val calls: RealChatRoomCallRepository,
    private val callController: CallController,
    private val chatEngine: ChatEngine,
) : AppInitializer {
    private val rungCalls = ConcurrentHashMap.newKeySet<String>()

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        scope.launch { ringNewCalls() }
    }

    /** Rings an offer that a push carried, before the worker of the product has it. */
    suspend fun ringPushedCall(chatId: ChatId, callId: String, callerName: String, video: Boolean) {
        if (!rungCalls.add(callId)) return
        callController.initiateIncomingCall(chatId, callId, callerName, video)
    }

    private suspend fun ringNewCalls() {
        calls.observeAll().collect { all ->
            all.forEach { (chatId, call) -> if (call is ChatRoomCallModel.Ringing) ring(chatId, call) }
        }
    }

    private suspend fun ring(chatId: ChatId, call: ChatRoomCallModel.Ringing) {
        if (!rungCalls.add(call.callId)) return
        val callerName = chatEngine.getChatDisplay(chatId).logFailure("No name for a ringing call").getOrNull()?.name.orEmpty()
        callController.initiateIncomingCall(chatId, call.callId, callerName, call.video)
    }
}
