package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.ChatHostBridge
import io.parity.truapi.HOST_DEFAULT_CHAT_ROOM_CONFIG
import io.parity.truapi.HOST_DEFAULT_CHAT_ROOM_FEATURES
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRejection
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import kotlinx.coroutines.flow.firstOrNull
import timber.log.Timber
import uniffi.truapi.ChatActivity
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatCommandSpec
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatMessageMeta
import uniffi.truapi.ChatReaction
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomCall
import uniffi.truapi.ChatRoomConfig
import uniffi.truapi.ChatRoomFeatures
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.ChatRoomUserState
import uniffi.truapi.ChatSuggestion
import uniffi.truapi.ChatToolbarAction
import uniffi.truapi.HostRejection
import uniffi.truapi.NativeChatFeedRejection

/**
 * Answers the core's chat callbacks for one product against its chat binding. The core has checked
 * that the product hosts each room and screened every value; the empty-id checks keep a broken call
 * away from the chat store. The user state of the rooms and the bot personas come from [storedState],
 * so they are available also when no chat screen is bound.
 */
class ProductChatHostBridge(
    private val productId: ProductId,
    private val chatMessaging: ProductChatMessaging,
    private val storedState: ProductChatStoredState,
) : ChatHostBridge {
    override suspend fun createRoom(roomId: String, name: String, icon: String): ChatRoomRegistrationStatus {
        if (roomId.isEmpty()) throw HostRejection.Rejected("a chat room needs an id")
        return chatMessaging.createRoom(CreateProductRoomRequest(ProductChatIdParameter(roomId), name, icon))
            .orReject()
            .status
            .toCoreStatus()
    }

    override suspend fun registerBot(botId: String, name: String, icon: String): ChatBotRegistrationStatus =
        storedState.registerBot(productId, botId, name, icon)

    override suspend fun postMessage(roomId: String, content: ChatMessageContent, alt: String?): String {
        if (roomId.isEmpty()) throw HostRejection.Rejected("a chat message needs a room")
        val room = ProductChatIdParameter(roomId)
        // A v1 reaction is a reaction of the product, applied as a feed update (02-DESIGN §13 item 10).
        when (content) {
            is ChatMessageContent.Reaction -> return postReaction(room, content.v1, placed = true)
            is ChatMessageContent.ReactionRemoved -> return postReaction(room, content.v1, placed = false)
            else -> Unit
        }
        val message = content.toProductBotMessage(alt)
            ?: throw HostRejection.Rejected("this host cannot apply a ${content.javaClass.simpleName} message")
        return chatMessaging.sendMessage(room, message).orReject()
    }

    override suspend fun setRoomFooter(roomId: String, footer: ChatRoomFooter) {
        chatMessaging.setRoomFooter(requireRoom(roomId), footer.toModel()).orReject()
    }

    override suspend fun listRooms(): List<ChatRoom> =
        chatMessaging.subscribeChatRooms().firstOrNull()?.map { it.toCoreChatRoom() } ?: emptyList()

    override suspend fun roomConfig(roomId: String): ChatRoomConfig =
        chatMessaging.roomDeclarations(requireRoom(roomId)).orReject()?.config?.toCore() ?: HOST_DEFAULT_CHAT_ROOM_CONFIG

    override suspend fun setRoomConfig(roomId: String, config: ChatRoomConfig) {
        chatMessaging.setRoomConfig(requireRoom(roomId), config.toModel()).orReject()
    }

    override suspend fun setRoomMetadata(roomId: String, metadata: ChatRoomMetadata) {
        chatMessaging.setRoomMetadata(requireRoom(roomId), metadata.name, metadata.toModel()).orReject()
    }

    override suspend fun setRoomPreview(roomId: String, preview: ChatRoomPreview) {
        chatMessaging.setRoomPreview(requireRoom(roomId), preview.toModel()).orReject()
    }

    override suspend fun deleteRoom(roomId: String) {
        chatMessaging.deleteRoom(requireRoom(roomId)).orReject()
    }

    /**
     * The core sets [persist] for a room that the host stores. For a room that the product keeps, only the
     * screens that show the room take the update.
     */
    override suspend fun updateFeed(roomId: String, update: ChatFeedUpdate, persist: Boolean) {
        if (roomId.isEmpty()) throw NativeChatFeedRejection.Rejected("a chat room needs an id")
        val room = ProductChatIdParameter(roomId)
        val model = update.toModel(rendererId = productId.toChatExtensionId())

        if (!persist) {
            if (!chatMessaging.deliverLiveFeedUpdate(room, model)) Timber.d("truapi.chat.updateFeed for %s: no open room", roomId)
            return
        }

        chatMessaging.applyFeedUpdate(room, model).getOrElse { throw it.toFeedRejection() }
    }

    override suspend fun setRoomToolbar(roomId: String, actions: List<ChatToolbarAction>) {
        chatMessaging.setRoomToolbar(requireRoom(roomId), actions.map { it.toModel() }).orReject()
    }

    override suspend fun setRoomSuggestions(roomId: String, suggestions: List<ChatSuggestion>) {
        chatMessaging.setRoomSuggestions(requireRoom(roomId), suggestions.map { it.toModel() }).orReject()
    }

    override suspend fun setRoomCommands(roomId: String, commands: List<ChatCommandSpec>) {
        chatMessaging.setRoomCommands(requireRoom(roomId), commands.map { it.toModel() }).orReject()
    }

    override suspend fun setRoomActivity(roomId: String, activity: ChatActivity?) {
        chatMessaging.setRoomActivity(requireRoom(roomId), activity?.toModel()).orReject()
    }

    // The emulator launcher compares this line with what the product reads (04-TESTING S49).
    override suspend fun capabilities(): List<String> = ChatHostCapabilities.features
        .also { Timber.i("truapi.chat.capabilities %s: %s", productId.value, it.joinToString(",")) }

    override suspend fun roomFeatures(roomId: String): ChatRoomFeatures =
        chatMessaging.roomDeclarations(requireRoom(roomId)).orReject()?.features?.toCore() ?: HOST_DEFAULT_CHAT_ROOM_FEATURES

    override suspend fun setRoomFeatures(roomId: String, features: ChatRoomFeatures) {
        chatMessaging.setRoomFeatures(requireRoom(roomId), features.toModel()).orReject()
    }

    override suspend fun setMessageMeta(roomId: String, messageId: String, meta: ChatMessageMeta) {
        if (roomId.isEmpty()) throw NativeChatFeedRejection.Rejected("a chat room needs an id")
        chatMessaging.setMessageMeta(ProductChatIdParameter(roomId), messageId, meta.toModel()).getOrElse { throw it.toFeedRejection() }
    }

    override suspend fun setRoomCall(roomId: String, call: ChatRoomCall?) {
        chatMessaging.setRoomCall(requireRoom(roomId), call?.toModel()).orReject()
    }

    override suspend fun roomUserStates(): List<ChatRoomUserState> = storedState.roomUserStates(productId)

    override suspend fun bots(): List<String> = storedState.botIds(productId)

    // A reaction is no message of its own: the answer names the message that it reacts to.
    private suspend fun postReaction(room: ProductChatIdParameter, reaction: ChatReaction, placed: Boolean): String {
        val update = if (placed) {
            ProductFeedUpdateModel.ReactionPlaced(reaction.messageId, ProductParticipant.PRODUCT, reaction.emoji)
        } else {
            ProductFeedUpdateModel.ReactionRemoved(reaction.messageId, ProductParticipant.PRODUCT, reaction.emoji)
        }
        chatMessaging.applyFeedUpdate(room, update).orReject()
        return reaction.messageId
    }

    private fun requireRoom(roomId: String): ProductChatIdParameter {
        if (roomId.isEmpty()) throw HostRejection.Rejected("a chat room needs an id")
        return ProductChatIdParameter(roomId)
    }

    private fun Throwable.toFeedRejection(): NativeChatFeedRejection = when (this) {
        is ProductFeedRejection.UnknownMessage -> NativeChatFeedRejection.UnknownMessage()
        is ProductFeedRejection.DuplicateMessage -> NativeChatFeedRejection.DuplicateMessage()
        is ProductFeedRejection.NotOwner -> NativeChatFeedRejection.NotOwner()
        is ProductFeedRejection.UnknownRoom -> NativeChatFeedRejection.Rejected("no such chat room")
        else -> NativeChatFeedRejection.Rejected(message ?: toString())
    }

    private fun <T> Result<T>.orReject(): T = getOrElse { failure ->
        when (failure) {
            is HostRejection -> throw failure
            is UnknownChatRoomException,
            is ProductFeedRejection.UnknownRoom,
            -> throw HostRejection.Rejected("no such chat room")
            is ProductFeedRejection -> throw HostRejection.Rejected(failure.message ?: failure.toString())
        }
        Timber.w(failure, "truapi.chat refused for %s", productId.value)
        throw HostRejection.Rejected(failure.message ?: failure.toString())
    }

    private fun CreateRoomStatus.toCoreStatus(): ChatRoomRegistrationStatus = when (this) {
        CreateRoomStatus.New -> ChatRoomRegistrationStatus.NEW
        CreateRoomStatus.Exists -> ChatRoomRegistrationStatus.EXISTS
    }
}
