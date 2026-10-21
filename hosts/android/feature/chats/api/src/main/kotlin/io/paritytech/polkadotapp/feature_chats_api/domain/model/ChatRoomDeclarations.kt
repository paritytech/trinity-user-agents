package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray

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
    val footer: ChatRoomFooterModel?,
    /** The features of the Chat Extension v2 additions. The host keeps them also when it does not serve them. */
    val features: ChatRoomFeaturesModel?,
) {
    /** The configuration that applies: the declared one, or the host default. */
    val effectiveConfig: ChatRoomConfigModel
        get() = config ?: ChatRoomConfigModel.HOST_DEFAULT

    /** The footer that applies: the declared one, or the input of the host. */
    val effectiveFooter: ChatRoomFooterModel
        get() = footer ?: ChatRoomFooterModel.Input(ChatInputConfigModel.HOST_DEFAULT)

    /** The features that apply: the declared ones, or none. */
    val effectiveFeatures: ChatRoomFeaturesModel
        get() = features ?: ChatRoomFeaturesModel.HOST_DEFAULT

    companion object {
        val EMPTY = ChatRoomDeclarations(
            config = null,
            configGeneration = 0,
            metadata = ChatRoomMetadataModel(description = null, avatar = null),
            preview = null,
            footer = null,
            features = null,
        )
    }
}

/** The parts of a room that the product serves itself (`ChatRoomConfig`). */
data class ChatRoomConfigModel(
    /** The product keeps the messages; the host keeps only a page cache. */
    val productSource: Boolean,
    /** The host offers "who reacted" and asks the product for it. */
    val reactionDetails: Boolean,
    /** The host asks the product for the participants. */
    val participants: Boolean,
    /** The host asks the product for the menu of a message. */
    val messageActions: Boolean,
    /** The emoji of the reaction picker. Null offers all emoji. */
    val allowedReactions: List<String>?,
    /** The text of a room without messages. Null shows the host text. */
    val placeholder: String?,
    /** The product wants a `Seen` event for the messages that the user saw. */
    val reportSeen: Boolean,
) {
    companion object {
        val HOST_DEFAULT = ChatRoomConfigModel(
            productSource = false,
            reactionDetails = false,
            participants = false,
            messageActions = false,
            allowedReactions = null,
            placeholder = null,
            reportSeen = false,
        )
    }
}

/** The description and the avatar of a room. The name stays with the room itself. */
data class ChatRoomMetadataModel(
    val description: String?,
    /** Null draws the icon of the product. */
    val avatar: ChatResourceModel?,
)

/** How a room shows in the chat list (`ChatRoomPreview`). */
data class ChatRoomPreviewModel(
    val order: Order,
    /** Milliseconds since the Unix epoch. Null uses the time of the last message. */
    val timestamp: Long?,
    /** Null keeps the unread count of the host. */
    val badge: ChatRoomBadgeModel?,
    /** Null shows the last message. */
    val text: ChatRoomPreviewTextModel?,
)

sealed interface ChatRoomBadgeModel {
    /** The host shows no badge. */
    data object Hidden : ChatRoomBadgeModel

    /** Zero shows no badge. */
    data class Count(val count: Int) : ChatRoomBadgeModel

    data class Image(val resource: ChatResourceModel) : ChatRoomBadgeModel
}

data class ChatRoomPreviewTextModel(
    val text: String,
    /** The text shows as an activity that continues, for example "Alice is typing". */
    val ongoing: Boolean,
)

/** Bytes that a product gives for an avatar, an icon or a media item (`ChatResource`). */
sealed interface ChatResourceModel {
    data class Inline(val mimeType: String, val bytes: DataByteArray) : ChatResourceModel

    /** The 32-byte key of a preimage that the host gets through the preimage lookup. */
    data class Preimage(val key: DataByteArray) : ChatResourceModel

    /** An address of a v1 message body (`https` or `data:`). A v2 declaration never carries it. */
    data class Url(val url: String) : ChatResourceModel

    /**
     * A file that a host sealed (`sealed_attachments`): XChaCha20-Poly1305 of the file under [key] and [nonce], stored
     * on the Bulletin chain as [parts], each the 32-byte key of one preimage. [size] is the length of the ciphertext and
     * [digest] its SHA-512.
     */
    data class Sealed(
        val parts: List<DataByteArray>,
        val key: DataByteArray,
        val nonce: DataByteArray,
        val size: Long,
        val digest: DataByteArray,
    ) : ChatResourceModel

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

/** What the host does when the user selects a product action (`ChatActionHandler`). */
sealed interface ChatActionHandlerModel {
    data class Deeplink(val url: String) : ChatActionHandlerModel

    /** The host sends the event of the action to the product. */
    data object Callback : ChatActionHandlerModel
}

/** What the room shows below its messages (`ChatRoomFooter`, second version). */
sealed interface ChatRoomFooterModel {
    data class Input(val config: ChatInputConfigModel) : ChatRoomFooterModel

    data object Empty : ChatRoomFooterModel

    /** The product draws the footer. */
    data object Custom : ChatRoomFooterModel
}

/** The input below the messages (`ChatInputConfig`). */
data class ChatInputConfigModel(
    /** The hint text of the empty input. Null shows the host text. */
    val placeholder: String?,
    val photo: Boolean,
    val video: Boolean,
    val file: Boolean,
    val payment: ChatInputPaymentModel?,
) {
    companion object {
        /** The input of a room that declares nothing: photo and video on, file and payment off. */
        val HOST_DEFAULT = ChatInputConfigModel(placeholder = null, photo = true, video = true, file = false, payment = null)
    }
}

sealed interface ChatInputPaymentModel {
    /** The 32 bytes of the account that gets the payment. */
    data class Enabled(val destination: DataByteArray) : ChatInputPaymentModel
}
