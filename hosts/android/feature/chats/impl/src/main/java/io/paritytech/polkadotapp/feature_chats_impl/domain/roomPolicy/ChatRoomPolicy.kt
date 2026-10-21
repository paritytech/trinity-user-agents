package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order

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
    val footer: Footer,
    val placeholder: String?,
    val description: String?,
    val avatar: ChatResourceModel?,
    val productQueries: Set<ProductQuery>,
    /** Changes at each configuration that the product sets, so a screen asks its questions again. */
    val configGeneration: Long,
    /**
     * The features of the Chat Extension v2 additions as the product declared them. A screen applies a feature only
     * when the host lists its name in `ChatHostCapabilities`.
     */
    val features: ChatRoomFeaturesModel,
    /** What the user set for the room: mute and archive. */
    val userState: ChatRoomUserStateModel,
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

        data class Image(val resource: ChatResourceModel) : ListBadge
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
    ) {
        val allowsAttachments: Boolean
            get() = allowsPhoto || allowsVideo || allowsFile
    }

    sealed interface Footer {
        data class TextInput(val input: Input) : Footer

        data object Empty : Footer

        data object Custom : Footer
    }

    enum class ProductQuery { PARTICIPANTS, MESSAGE_ACTIONS }

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
            footer = Footer.TextInput(DefaultInput),
            placeholder = null,
            description = null,
            avatar = null,
            productQueries = emptySet(),
            configGeneration = 0,
            features = ChatRoomFeaturesModel.HOST_DEFAULT,
            userState = ChatRoomUserStateModel.NONE,
        )

        fun from(declarations: ChatRoomDeclarations?, userState: ChatRoomUserStateModel = ChatRoomUserStateModel.NONE): ChatRoomPolicy {
            if (declarations == null) return Defaults.copy(userState = userState)

            val config = declarations.effectiveConfig

            return ChatRoomPolicy(
                messageStore = if (config.productSource) MessageStore.PRODUCT else MessageStore.HOST,
                listPreview = declarations.preview.toListPreview(),
                listBadge = declarations.preview?.badge.toListBadge(),
                listOrder = ListOrder(
                    isPinned = declarations.preview?.order == Order.PinToTop,
                    timestamp = declarations.preview?.timestamp?.takeIf { it > 0 },
                ),
                reportsSeen = config.reportSeen,
                messageMenu = MessageMenu(
                    builtIns = Defaults.messageMenu.builtIns,
                    allowedReactions = config.allowedReactions,
                ),
                offersReactionDetails = config.reactionDetails,
                footer = declarations.footer.toFooter(),
                placeholder = config.placeholder,
                description = declarations.metadata.description,
                avatar = declarations.metadata.avatar,
                productQueries = config.productQueries(),
                configGeneration = declarations.configGeneration,
                features = declarations.effectiveFeatures,
                userState = userState,
            )
        }
    }
}

private fun ChatRoomPreviewModel?.toListPreview(): ChatRoomPolicy.ListPreview {
    val text = this?.text ?: return ChatRoomPolicy.ListPreview.LastMessage

    return ChatRoomPolicy.ListPreview.Text(text = text.text, ongoing = text.ongoing)
}

private fun ChatRoomBadgeModel?.toListBadge(): ChatRoomPolicy.ListBadge {
    return when (this) {
        null -> ChatRoomPolicy.ListBadge.HostUnreadCount
        ChatRoomBadgeModel.Hidden -> ChatRoomPolicy.ListBadge.Hidden
        is ChatRoomBadgeModel.Count -> ChatRoomPolicy.ListBadge.Count(count.coerceAtLeast(0))
        is ChatRoomBadgeModel.Image -> ChatRoomPolicy.ListBadge.Image(resource)
    }
}

private fun ChatRoomFooterModel?.toFooter(): ChatRoomPolicy.Footer {
    return when (this) {
        null -> ChatRoomPolicy.Defaults.footer
        ChatRoomFooterModel.Empty -> ChatRoomPolicy.Footer.Empty
        ChatRoomFooterModel.Custom -> ChatRoomPolicy.Footer.Custom
        is ChatRoomFooterModel.Input -> ChatRoomPolicy.Footer.TextInput(
            ChatRoomPolicy.Input(
                allowsPhoto = config.photo,
                allowsVideo = config.video,
                allowsFile = config.file,
                allowsPayment = config.payment != null,
                placeholder = config.placeholder,
            )
        )
    }
}

private fun ChatRoomConfigModel.productQueries(): Set<ChatRoomPolicy.ProductQuery> {
    return buildSet {
        if (participants) add(ChatRoomPolicy.ProductQuery.PARTICIPANTS)
        if (messageActions) add(ChatRoomPolicy.ProductQuery.MESSAGE_ACTIONS)
    }
}
