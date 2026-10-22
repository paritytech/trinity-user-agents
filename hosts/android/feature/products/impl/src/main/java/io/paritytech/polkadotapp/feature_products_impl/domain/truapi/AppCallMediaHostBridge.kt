package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.parity.truapi.CallMediaHostBridge
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallMedia
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallSignals
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductMediaCall
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import timber.log.Timber
import uniffi.truapi.ChatRoomEvent
import uniffi.truapi.HostRejection
import javax.inject.Inject

/**
 * The media of the calls of product rooms, on the call engine and the call screen of the contact calls: WebRTC with
 * the TURN relays of the backend. The product carries the signals between the two devices through its own encrypted
 * channel, so the media is end to end. The core allows a start only shortly after the user pressed a call button or
 * accepted a call in the room.
 */
class AppCallMediaHostBridge @Inject constructor(
    private val callMedia: ProductCallMedia,
    private val rooms: ReadOnlyChatRoomRepository,
    private val roomDeclarations: ChatRoomDeclarationsRepository,
    private val fallbackUsernameGenerator: FallbackUsernameGenerator,
    // Lazy: the runtime provider installs this bridge.
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
) : CallMediaHostBridge {
    override suspend fun startCallMedia(productId: String, roomId: String, callId: String, video: Boolean, offerer: Boolean) {
        val chatId = chatIdOf(productId, roomId)
        // A person chat calls with its contact, under the name the native chat gives the contact.
        val roomName = roomDeclarations.get(chatId)?.contact?.let { it.username ?: fallbackUsernameGenerator.generateFromAccountId(it.account) }
            ?: runCancellableCatching { rooms.getRoomMetadata(chatId).name }.getOrNull().orEmpty()
        val call = ProductMediaCall(chatId, callId, roomName, video, offerer)
        callMedia.start(call, RoomSignals(productId, roomId, callId)).onFailure {
            throw HostRejection.Rejected(it.message ?: "the call did not start")
        }
    }

    override suspend fun callMediaSignal(productId: String, roomId: String, callId: String, signal: ByteArray) {
        callMedia.signal(chatIdOf(productId, roomId), callId, signal)
    }

    override suspend fun stopCallMedia(productId: String, roomId: String, callId: String) {
        callMedia.stop(chatIdOf(productId, roomId), callId)
    }

    private fun chatIdOf(productId: String, roomId: String): ChatId =
        ProductChatIdParameter(roomId).toChatId(ProductId.fromStoredValue(productId).toChatExtensionId())

    private inner class RoomSignals(
        private val productId: String,
        private val roomId: String,
        private val callId: String,
    ) : ProductCallSignals {
        override suspend fun send(signal: ByteArray) = publish(ChatRoomEvent.CallSignal(callId, signal))

        override suspend fun ended() = publish(ChatRoomEvent.CallEnded(callId))

        // A call that ends while the runtime goes away has nobody to tell.
        private suspend fun publish(event: ChatRoomEvent) {
            runtimeProvider.get().runtime()
                .mapCatching { it.publishChatRoomEvent(productId, roomId, event) }
                .onFailure { Timber.w(it, "Product call %s: the event did not reach the product", callId) }
        }
    }
}
