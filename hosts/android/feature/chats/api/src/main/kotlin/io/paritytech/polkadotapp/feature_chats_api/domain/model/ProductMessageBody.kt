package io.paritytech.polkadotapp.feature_chats_api.domain.model

/**
 * A message body of a product that the native kinds cannot hold: rich text with product media, or
 * host-drawn buttons. A text body stays [ChatMessage.Content.Text], and a body that the product
 * draws stays [ChatMessage.Content.Custom].
 */
sealed interface ProductMessageBody {
    /** [text] is Markdown. The media draw below it. */
    data class RichText(val text: String?, val media: List<ChatMediaItemModel>) : ProductMessageBody

    /** A press on a button sends `ActionTriggered` to the product. */
    data class Actions(val text: String?, val actions: List<ChatActionButtonModel>, val layout: ChatActionsLayoutModel) :
        ProductMessageBody
}

sealed interface ChatMediaItemModel {
    val resource: ChatResourceModel
    val meta: ChatMediaMetaModel

    data class Image(override val resource: ChatResourceModel, override val meta: ChatMediaMetaModel) : ChatMediaItemModel

    data class Video(override val resource: ChatResourceModel, override val meta: ChatMediaMetaModel) : ChatMediaItemModel

    data class File(
        override val resource: ChatResourceModel,
        override val meta: ChatMediaMetaModel,
        val fileName: String,
    ) : ChatMediaItemModel
}

/** What the timeline needs before the bytes arrive (`ChatMediaMeta`). */
data class ChatMediaMetaModel(
    val mimeType: String,
    /** The product gives it. The host verifies it only for a preimage that it gets. */
    val sizeBytes: Long?,
    val width: Int?,
    val height: Int?,
    /** A BlurHash that the host draws until the bytes arrive. */
    val placeholder: String?,
    /** The accessibility text, and the list preview of a message with media only. */
    val alt: String?,
)

data class ChatActionButtonModel(val actionId: String, val title: String)

enum class ChatActionsLayoutModel { COLUMN, GRID }

/** The text that the chat list, a notification or a reply shows for the body. */
fun ProductMessageBody.previewText(): String? = when (this) {
    is ProductMessageBody.RichText -> text?.takeIf { it.isNotBlank() } ?: media.firstNotNullOfOrNull { it.meta.alt }
    is ProductMessageBody.Actions -> text?.takeIf { it.isNotBlank() } ?: actions.firstOrNull()?.title
}
