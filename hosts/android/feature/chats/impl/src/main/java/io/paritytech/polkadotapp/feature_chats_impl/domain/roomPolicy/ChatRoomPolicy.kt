package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResource
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadge
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomOrder
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomToolbarAction

/**
 * What a product room offers. Each aspect comes from the declarations of the product or, when the product
 * declares nothing for it, from the host defaults of 02-DESIGN §4.
 */
data class ChatRoomPolicy(
    val messageStore: MessageStore,
    val listPreview: ListPreview,
    val listBadge: ListBadge,
    val listOrder: ListOrder,
    val reportsSeen: Boolean,
    val messageMenu: MessageMenu,
    val offersReactionDetails: Boolean,
    val toolbar: List<ChatRoomToolbarAction>,
    val footer: Footer,
    val placeholder: String?,
    val description: String?,
    val avatar: ChatResource?,
    val suggestions: List<String>,
    val slashCommands: Set<String>,
    val productQueries: Set<ProductQuery>,
) {
    enum class MessageStore {
        /** The host database keeps the messages. */
        HOST,

        /** The product keeps the messages, and the host keeps a page cache. */
        PRODUCT,
    }

    sealed interface ListPreview {
        data object LastMessage : ListPreview

        /** When [ongoing] is true, the row shows the text as an activity that continues. */
        data class Text(val text: String, val ongoing: Boolean) : ListPreview
    }

    sealed interface ListBadge {
        data object HostUnreadCount : ListBadge

        data object Hidden : ListBadge

        /** Zero shows no badge. */
        data class Count(val count: Int) : ListBadge

        data class Image(val resource: ChatResource) : ListBadge
    }

    /** [timestamp] is in milliseconds since the Unix epoch. Null uses the last message. */
    data class ListOrder(val isPinned: Boolean, val timestamp: Long?)

    enum class BuiltInMessageAction { COPY, REPLY, EDIT, DELETE, REACT, RETRY }

    /** Null [allowedReactions] offers all emoji. */
    data class MessageMenu(val builtIns: Set<BuiltInMessageAction>, val allowedReactions: List<String>?)

    data class Input(
        val allowsPhoto: Boolean,
        val allowsVideo: Boolean,
        val allowsFile: Boolean,
        val allowsPayment: Boolean,
        val placeholder: String?,
    )

    sealed interface Footer {
        data class TextInput(val input: Input) : Footer

        data object Empty : Footer

        data object Custom : Footer
    }

    enum class ProductQuery { PARTICIPANTS, MESSAGE_ACTIONS, PARTICIPANT_ACTIONS }

    /** The built-in Retry exists only in a room whose product keeps the messages (02-DESIGN §13 item 9). */
    val offersRetry: Boolean
        get() = messageStore == MessageStore.PRODUCT && BuiltInMessageAction.RETRY in messageMenu.builtIns

    val offersDelete: Boolean
        get() = BuiltInMessageAction.DELETE in messageMenu.builtIns

    /** The host applies reply, edit, delete and reactions to its own store and reports them to the product. */
    val hostAppliesUserActions: Boolean
        get() = messageStore == MessageStore.HOST

    /** A product badge replaces the host count and the reaction mark (02-DESIGN §19 item 3). */
    val replacesHostBadge: Boolean
        get() = listBadge != ListBadge.HostUnreadCount

    companion object {
        val DefaultInput = Input(
            allowsPhoto = true,
            allowsVideo = true,
            allowsFile = false,
            allowsPayment = false,
            placeholder = null,
        )

        /** The policy of a product room that declares nothing. */
        val Defaults = ChatRoomPolicy(
            messageStore = MessageStore.HOST,
            listPreview = ListPreview.LastMessage,
            listBadge = ListBadge.HostUnreadCount,
            listOrder = ListOrder(isPinned = false, timestamp = null),
            reportsSeen = false,
            messageMenu = MessageMenu(builtIns = BuiltInMessageAction.entries.toSet(), allowedReactions = null),
            offersReactionDetails = false,
            toolbar = emptyList(),
            footer = Footer.TextInput(DefaultInput),
            placeholder = null,
            description = null,
            avatar = null,
            suggestions = emptyList(),
            slashCommands = emptySet(),
            productQueries = emptySet(),
        )

        fun from(declarations: ChatRoomDeclarations?): ChatRoomPolicy {
            if (declarations == null) return Defaults

            val config = declarations.config

            return ChatRoomPolicy(
                messageStore = if (config.productSource) MessageStore.PRODUCT else MessageStore.HOST,
                listPreview = declarations.preview.toListPreview(),
                listBadge = declarations.preview?.badge.toListBadge(),
                listOrder = ListOrder(
                    isPinned = declarations.preview?.order == ChatRoomOrder.PIN_TO_TOP,
                    timestamp = declarations.preview?.timestamp?.takeIf { it > 0 },
                ),
                reportsSeen = config.reportSeen,
                messageMenu = MessageMenu(
                    builtIns = Defaults.messageMenu.builtIns,
                    allowedReactions = config.allowedReactions,
                ),
                offersReactionDetails = config.reactionDetails,
                toolbar = declarations.toolbar,
                footer = declarations.footer.toFooter(),
                placeholder = config.placeholder,
                description = declarations.metadata.description,
                avatar = declarations.metadata.avatar,
                suggestions = declarations.suggestions.map { it.text },
                slashCommands = declarations.commands.mapTo(mutableSetOf()) { it.name },
                productQueries = config.productQueries(),
            )
        }
    }
}

private fun ChatRoomPreview?.toListPreview(): ChatRoomPolicy.ListPreview {
    val text = this?.text ?: return ChatRoomPolicy.ListPreview.LastMessage

    return ChatRoomPolicy.ListPreview.Text(text = text.text, ongoing = text.ongoing)
}

private fun ChatRoomBadge?.toListBadge(): ChatRoomPolicy.ListBadge {
    return when (this) {
        null -> ChatRoomPolicy.ListBadge.HostUnreadCount
        ChatRoomBadge.Hidden -> ChatRoomPolicy.ListBadge.Hidden
        is ChatRoomBadge.Count -> ChatRoomPolicy.ListBadge.Count(count.coerceAtLeast(0))
        is ChatRoomBadge.Image -> ChatRoomPolicy.ListBadge.Image(resource)
    }
}

private fun ChatRoomFooterConfig?.toFooter(): ChatRoomPolicy.Footer {
    return when (this) {
        null -> ChatRoomPolicy.Defaults.footer
        ChatRoomFooterConfig.Empty -> ChatRoomPolicy.Footer.Empty
        ChatRoomFooterConfig.Custom -> ChatRoomPolicy.Footer.Custom
        is ChatRoomFooterConfig.Input -> ChatRoomPolicy.Footer.TextInput(
            ChatRoomPolicy.Input(
                allowsPhoto = config.photo,
                allowsVideo = config.video,
                allowsFile = config.file,
                allowsPayment = config.paymentDestination != null,
                placeholder = config.placeholder,
            )
        )
    }
}

private fun ChatRoomConfig.productQueries(): Set<ChatRoomPolicy.ProductQuery> {
    return buildSet {
        if (participants) add(ChatRoomPolicy.ProductQuery.PARTICIPANTS)
        if (messageActions) add(ChatRoomPolicy.ProductQuery.MESSAGE_ACTIONS)
        if (participantActions) add(ChatRoomPolicy.ProductQuery.PARTICIPANT_ACTIONS)
    }
}
