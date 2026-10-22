package io.paritytech.polkadotapp.feature_chats_api.domain.model

/**
 * A message body of a product that the native kinds cannot hold: plain text with product media. A text body stays
 * [ChatMessage.Content.Text], and a body that the product draws stays [ChatMessage.Content.Custom].
 */
sealed interface ProductMessageBody {
    /** [text] is plain text, like a text body. The media draw with it. */
    data class RichText(val text: String?, val media: List<ChatMediaItemModel>) : ProductMessageBody
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
    val sizeBytes: Long?,
    val width: Int?,
    val height: Int?,
    /** A BlurHash that the host draws until the bytes arrive. */
    val placeholder: String?,
    /** The accessibility text. */
    val alt: String?,
    /** The length of a video, in milliseconds. */
    val durationMs: Long? = null,
)

/** The text that a reply quote or a notification shows for the body. */
fun ProductMessageBody.previewText(): String? = when (this) {
    is ProductMessageBody.RichText -> text?.takeIf { it.isNotBlank() } ?: media.firstNotNullOfOrNull { it.meta.alt }
}
