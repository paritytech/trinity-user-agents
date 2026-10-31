package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage

/**
 * What a product room offers. Each aspect comes from the declarations of the product or, when the product
 * declares nothing for it, from the host defaults.
 */
data class ChatRoomPolicy(
    val messageStore: MessageStore,
    /** The newest message and reaction that the product gave for the chat list, or null to show the host's last message. */
    val listPreview: ListPreview?,
    val listBadge: ListBadge,
    /** Milliseconds since the Unix epoch. Null uses the last message. */
    val listTimestamp: Long?,
    /** The chat list shows the room with the pinned rooms. */
    val pinned: Boolean,
    /** The line that the row shows in place of its preview line. */
    val listActivity: ChatRoomActivityModel?,
    val reportsSeen: Boolean,
    val description: String?,
    /** The text that the room screen shows while the room has no messages. */
    val placeholder: String?,
    /** Changes at each configuration that the product sets. */
    val configGeneration: Long,
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
        /** The reactions that the user did not see yet. The row shows the heart. */
        val unseenReactions: Int,
    )

    sealed interface ListBadge {
        data object HostUnreadCount : ListBadge

        /** Zero shows no badge. */
        data class Count(val count: Int) : ListBadge

        data class Image(val icon: String) : ListBadge
    }

    companion object {
        /** The policy of a product room that declares nothing. */
        val Defaults = ChatRoomPolicy(
            messageStore = MessageStore.HOST,
            listPreview = null,
            listBadge = ListBadge.HostUnreadCount,
            listTimestamp = null,
            pinned = false,
            listActivity = null,
            reportsSeen = false,
            description = null,
            placeholder = null,
            configGeneration = 0,
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
                pinned = preview?.pinned == true,
                listActivity = preview?.activity,
                reportsSeen = config.reportSeen,
                description = declarations.metadata.description,
                placeholder = declarations.metadata.placeholder,
                configGeneration = declarations.configGeneration,
            )
        }
    }
}

private fun ChatRoomBadgeModel?.toListBadge(): ChatRoomPolicy.ListBadge {
    return when (this) {
        null -> ChatRoomPolicy.ListBadge.HostUnreadCount
        is ChatRoomBadgeModel.Count -> ChatRoomPolicy.ListBadge.Count(count.coerceAtLeast(0))
        is ChatRoomBadgeModel.Image -> ChatRoomPolicy.ListBadge.Image(icon)
    }
}
