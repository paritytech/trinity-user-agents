package io.paritytech.polkadotapp.feature_chats_impl.data.migrations

import io.paritytech.polkadotapp.database.migrations.ChatMessageContentMigration
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.ContentV2
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.ContentV3
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.LegacyMultimediaType
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.AttachmentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.AttachmentMetaLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.RichTextContentLocal

/**
 * Content schema migration V2 → V3.
 * - Introduces RichText content type with attachments support
 * - Maps legacy Media and File content types to RichText with Embedded attachments
 * - Migrates ChatRequest.welcome from legacy RichTextContentLocal (media) to current (attachments)
 */
fun createChatMessageMigration2to3(): ChatMessageContentMigration<*, *> = ChatMessageContentMigration(
    versionFrom = 31,
    versionTo = 32,
    oldSerializer = ContentV2.serializer(),
    newSerializer = ContentV3.serializer(),
    mapper = { old ->
        when (old) {
            is ContentV2.Text -> ContentV3.Text(old.text)
            is ContentV2.Token -> ContentV3.Token(old.token, old.platform)
            ContentV2.ContactAdded -> ContentV3.ContactAdded
            is ContentV2.Reacted -> ContentV3.Reacted(old.messageId, old.content)
            is ContentV2.ReactionRemoved -> ContentV3.ReactionRemoved(old.messageId, old.content)
            is ContentV2.Media -> old.toRichText()
            is ContentV2.Unsupported -> ContentV3.Unsupported(old.rawContent)
            is ContentV2.File -> old.toRichText()
            ContentV2.LeftChat -> ContentV3.LeftChat
            is ContentV2.Edited -> ContentV3.Edited(old.messageId, old.newText)
            is ContentV2.DataChannelOffer -> ContentV3.DataChannelOffer(old.sdp, old.purpose)
            is ContentV2.DataChannelAnswer -> ContentV3.DataChannelAnswer(old.offerMessageId, old.sdp)
            is ContentV2.DataChannelIceCandidate -> ContentV3.DataChannelIceCandidate(old.offerMessageId, old.sdp)
            is ContentV2.ChatAccepted -> ContentV3.ChatAccepted(old.requestId)
            is ContentV2.ChatRequest -> ContentV3.ChatRequest(
                welcome = old.welcome?.let { RichTextContentLocal(text = it.text, attachments = null) }
            )

            is ContentV2.CoinagePayment -> ContentV3.CoinagePayment(
                totalValue = old.totalValue,
                coinKeys = old.coinKeys,
                status = old.status,
            )

            is ContentV2.DataChannelClosed -> ContentV3.DataChannelClosed(old.offerMessageId)
            is ContentV2.Custom -> ContentV3.Custom(old.rendererId, old.rawContent)
        }
    }
)

private fun ContentV2.Media.toRichText(): ContentV3.RichText {
    val meta = when (type) {
        LegacyMultimediaType.IMAGE -> {
            AttachmentMetaLocal.Image(
                width = 1280,
                height = 860,
                mimeType = "image/*",
                sizeBytes = 0,
                blurHash = null
            )
        }

        LegacyMultimediaType.VIDEO -> {
            AttachmentMetaLocal.Video(
                duration = 0,
                mimeType = "video/*",
                sizeBytes = 0,
                blurHash = null
            )
        }
    }

    return ContentV3.RichText(
        RichTextContentLocal(
            text = text,
            attachments = listOf(
                AttachmentLocal.Embedded(uri = url, meta = meta)
            )
        )
    )
}

private fun ContentV2.File.toRichText(): ContentV3.RichText {
    return ContentV3.RichText(
        RichTextContentLocal(
            text = text,
            attachments = listOf(
                AttachmentLocal.Embedded(
                    uri = url,
                    meta = AttachmentMetaLocal.General(fileName = fileName, mimeType = mimeType, sizeBytes = sizeBytes)
                )
            )
        )
    )
}
