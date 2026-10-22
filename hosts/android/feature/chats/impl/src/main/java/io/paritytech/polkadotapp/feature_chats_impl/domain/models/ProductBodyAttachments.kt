package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import android.net.Uri
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import kotlin.time.Duration.Companion.milliseconds

/**
 * The native rich text of a product body, so a product message draws as a message of a person chat: each media item is
 * an attachment, and the room draws the first one as the native chat does. [productMedia] holds the local files of the
 * media that loaded.
 */
fun ChatMessage.Content.ProductBody.toRichText(productMedia: Map<ChatResourceModel, Uri>): ChatMessage.Content.RichText {
    val body = body as ProductMessageBody.RichText
    val attachments = body.media.map { it.toAttachment(productMedia[it.resource]) }
    return ChatMessage.Content.RichText(text = body.text?.takeIf { it.isNotEmpty() }, attachments = attachments)
}

private fun ChatMediaItemModel.toAttachment(uri: Uri?): Attachment {
    val size = meta.sizeBytes?.bytes ?: InformationSize.ZERO
    val attachmentMeta = when (this) {
        is ChatMediaItemModel.Image -> Attachment.Meta.Image(
            width = meta.width ?: 0,
            height = meta.height ?: 0,
            blurHash = meta.placeholder,
            mimeType = meta.mimeType,
            size = size,
        )

        // The native chat keeps the length of a video in whole seconds.
        is ChatMediaItemModel.Video -> Attachment.Meta.Video(
            duration = meta.durationMs?.milliseconds?.inWholeSeconds ?: 0,
            blurHash = meta.placeholder,
            mimeType = meta.mimeType,
            size = size,
        )

        is ChatMediaItemModel.File -> Attachment.Meta.General(fileName = fileName, mimeType = meta.mimeType, size = size)
    }

    return when (val resource = resource) {
        is ChatResourceModel.Hop -> Attachment.Hosted(
            uri = uri,
            identifier = resource.identifier,
            ticket = HopTicket(resource.ticket),
            nodeUrl = resource.node,
            meta = attachmentMeta,
        )

        // The kept file of an attachment that waits in the outbox, or a v1 address.
        is ChatResourceModel.Url -> Attachment.Hosted(
            uri = uri,
            identifier = DataByteArray(ByteArray(0)),
            ticket = HopTicket(DataByteArray(ByteArray(0))),
            nodeUrl = resource.url,
            meta = attachmentMeta,
        )
    }
}
