package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage

/**
 * What a product room offers. Each aspect comes from the declarations of the product or, when the product
 * declares nothing for it, from the host defaults of 02-DESIGN §4.
 */
data class ChatRoomPolicy(
    val messageStore: MessageStore,
    /** The newest message and reaction that the product gave for the chat list, or null to show the host's last message. */
    val listPreview: ListPreview?,
    val listBadge: ListBadge,
    /** Milliseconds since the Unix epoch. Null uses the last message. */
    val listTimestamp: Long?,
    val reportsSeen: Boolean,
    val description: String?,
    /** Changes at each configuration that the product sets. */
    val configGeneration: Long,
    /**
     * The features of the Chat Extension v2 additions as the product declared them. A screen applies a feature only
     * when the host lists its name in `ChatHostCapabilities`.
     */
    val features: ChatRoomFeaturesModel,
    /** The person the room is a conversation with. The host draws such a room as its own person chat. */
    val contact: ChatRoomContactModel?,
) {
    enum class MessageStore {
        /** The host database keeps the messages. */
        HOST,

        /** The product keeps the messages, and the host keeps a page cache. */
        PRODUCT,
    }

    data class ListPreview(
        val lastMessage: ProductRoomMessage?,
        val lastReaction: ChatPreviewReactionModel?,
        /**
         * The reactions of the contact that the user did not see yet. The row shows the heart, and the badge counts
         * them, as the native chat counts each unseen reaction message.
         */
        val unseenReactions: Int,
    )

    sealed interface ListBadge {
        data object HostUnreadCount : ListBadge

        data object Hidden : ListBadge

        /** Zero shows no badge. */
        data class Count(val count: Int) : ListBadge
    }

    /** The host applies reply, edit and reactions to its own store and reports them to the product. */
    val hostAppliesUserActions: Boolean
        get() = messageStore == MessageStore.HOST

    /** A product badge replaces the host count and the reaction mark (02-DESIGN §19 item 3). */
    val replacesHostBadge: Boolean
        get() = listBadge != ListBadge.HostUnreadCount

    /** The room is a person chat of the chat product, which the host lists among its person chats. */
    val isListedContactRoom: Boolean
        get() = contact?.state?.isListed() == true

    companion object {
        /** The policy of a product room that declares nothing. */
        val Defaults = ChatRoomPolicy(
            messageStore = MessageStore.HOST,
            listPreview = null,
            listBadge = ListBadge.HostUnreadCount,
            listTimestamp = null,
            reportsSeen = false,
            description = null,
            configGeneration = 0,
            features = ChatRoomFeaturesModel.HOST_DEFAULT,
            contact = null,
        )

        fun from(declarations: ChatRoomDeclarations?): ChatRoomPolicy {
            if (declarations == null) return Defaults

            val config = declarations.effectiveConfig
            val preview = declarations.preview

            return ChatRoomPolicy(
                messageStore = if (config.productSource) MessageStore.PRODUCT else MessageStore.HOST,
                listPreview = preview?.let { ListPreview(it.lastMessage, it.lastReaction, it.unseenReactions) },
                listBadge = preview?.badge.toListBadge(),
                listTimestamp = preview?.timestamp?.takeIf { it > 0 },
                reportsSeen = config.reportSeen,
                description = declarations.metadata.description,
                configGeneration = declarations.configGeneration,
                features = declarations.effectiveFeatures,
                contact = declarations.contact,
            )
        }
    }
}

/** The native chat lists an incoming request only behind "New requests", and neither an invite nor a blocked contact. */
fun ChatContactStateModel.isListed(): Boolean = when (this) {
    ChatContactStateModel.OUTGOING,
    ChatContactStateModel.ESTABLISHED,
    ChatContactStateModel.PEER_LEFT,
    -> true

    ChatContactStateModel.INVITE,
    ChatContactStateModel.INCOMING,
    ChatContactStateModel.BLOCKED,
    -> false
}

private fun ChatRoomBadgeModel?.toListBadge(): ChatRoomPolicy.ListBadge {
    return when (this) {
        null -> ChatRoomPolicy.ListBadge.HostUnreadCount
        ChatRoomBadgeModel.Hidden -> ChatRoomPolicy.ListBadge.Hidden
        is ChatRoomBadgeModel.Count -> ChatRoomPolicy.ListBadge.Count(count.coerceAtLeast(0))
    }
}
