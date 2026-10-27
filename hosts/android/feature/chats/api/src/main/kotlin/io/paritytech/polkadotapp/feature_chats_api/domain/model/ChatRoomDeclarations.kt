package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage

/**
 * What a product declared for one of its rooms (Chat Extension v2). The host keeps it, so the chat
 * list and the room screen can draw the room without the product worker. A null part means that the
 * product declared nothing for it, and the host default applies.
 */
data class ChatRoomDeclarations(
    val config: ChatRoomConfigModel?,
    /** Increases by one at each configuration that the product sets. */
    val configGeneration: Long,
    val metadata: ChatRoomMetadataModel,
    val preview: ChatRoomPreviewModel?,
) {
    /** The configuration that applies: the declared one, or the host default. */
    val effectiveConfig: ChatRoomConfigModel
        get() = config ?: ChatRoomConfigModel.HOST_DEFAULT

    companion object {
        val EMPTY = ChatRoomDeclarations(
            config = null,
            configGeneration = 0,
            metadata = ChatRoomMetadataModel(description = null),
            preview = null,
        )
    }
}

/** The parts of a room that the product serves itself (`ChatRoomConfig`). */
data class ChatRoomConfigModel(
    /** The product keeps the messages; the host keeps only a page cache. */
    val productSource: Boolean,
    /** The product answers who reacted. This host does not show it. */
    val reactionDetails: Boolean,
    /** The product wants a `Seen` event for the messages that the user saw. */
    val reportSeen: Boolean,
) {
    companion object {
        val HOST_DEFAULT = ChatRoomConfigModel(productSource = false, reactionDetails = false, reportSeen = false)
    }
}

/** The description of a room. The name stays with the room itself. */
data class ChatRoomMetadataModel(val description: String?)

/** How a room shows in the chat list (`ChatRoomPreview`). */
data class ChatRoomPreviewModel(
    /** Milliseconds since the Unix epoch. Null uses the time of the last message. */
    val timestamp: Long?,
    /** Null keeps the unread count of the host. */
    val badge: ChatRoomBadgeModel?,
    /** The newest message that the user sees in the room. The row draws its preview line from it. */
    val lastMessage: ProductRoomMessage?,
    /** The newest event of the room when it is a reaction newer than [lastMessage]. */
    val lastReaction: ChatPreviewReactionModel?,
    /** The number of reactions that the user did not see yet. */
    val unseenReactions: Int,
)

/** A reaction that is the newest event of a room (`ChatPreviewReaction`). */
data class ChatPreviewReactionModel(
    val emoji: String,
    /** True: the user reacted. False: another participant of the room did. */
    val byUser: Boolean,
    /** The message the reaction is on, when the product still has it. */
    val message: ProductRoomMessage?,
    /** The newest event is the removal of this reaction. */
    val removed: Boolean,
)

sealed interface ChatRoomBadgeModel {
    /** The host shows no badge. */
    data object Hidden : ChatRoomBadgeModel

    /** Zero shows no badge. */
    data class Count(val count: Int) : ChatRoomBadgeModel
}
