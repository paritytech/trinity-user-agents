package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
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
    /** The features of the Chat Extension v2 additions. The host keeps them also when it does not serve them. */
    val features: ChatRoomFeaturesModel?,
    /** The person the room is a conversation with. The host draws such a room as its own person chat. */
    val contact: ChatRoomContactModel?,
) {
    /** The configuration that applies: the declared one, or the host default. */
    val effectiveConfig: ChatRoomConfigModel
        get() = config ?: ChatRoomConfigModel.HOST_DEFAULT

    /** The features that apply: the declared ones, or none. */
    val effectiveFeatures: ChatRoomFeaturesModel
        get() = features ?: ChatRoomFeaturesModel.HOST_DEFAULT

    companion object {
        val EMPTY = ChatRoomDeclarations(
            config = null,
            configGeneration = 0,
            metadata = ChatRoomMetadataModel(description = null),
            preview = null,
            features = null,
            contact = null,
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
    /** The number of reactions of the contact that the user did not see yet. */
    val unseenReactions: Int,
)

/** A reaction that is the newest event of a room (`ChatPreviewReaction`). */
data class ChatPreviewReactionModel(
    val emoji: String,
    /** True: the user reacted. False: the contact of the room did. */
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

/** The person a room of the chat product is a conversation with (`ChatRoomContact`). */
data class ChatRoomContactModel(
    val account: AccountId,
    val username: String?,
    val state: ChatContactStateModel,
)

/** Where the conversation of a room with its contact stands (`ChatContactState`). */
enum class ChatContactStateModel {
    /** No request yet: the input is the invite composer. The room is not listed. */
    INVITE,

    /** The request of the user waits for the contact. */
    OUTGOING,

    /** The request of the contact waits for the user. The room is listed under the requests only. */
    INCOMING,
    ESTABLISHED,

    /** The user blocked the contact. The room is not listed. */
    BLOCKED,

    /** The contact left the chat. */
    PEER_LEFT,
}

/** Bytes that a product gives for a media item (`ChatResource`). */
sealed interface ChatResourceModel {
    /** An address of a v1 message body (`https` or `data:`). A v2 declaration never carries it. */
    data class Url(val url: String) : ChatResourceModel

    /**
     * A file of the chat product in the HOP pool of the native chat: the root entry [identifier] on [node], claimed and
     * opened with [ticket]. [size] is the size the sender states.
     */
    data class Hop(
        val identifier: DataByteArray,
        val ticket: DataByteArray,
        val node: String,
        val size: Long,
    ) : ChatResourceModel
}
