package io.paritytech.polkadotapp.feature_chats_impl.data.migrations

import io.paritytech.polkadotapp.database.migrations.ChatMessageContentMigration
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.ContentV3
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.ContentV4
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.RichTextContentLocal

/**
 * Content schema migration V3 → V4.
 * - Now Edited content type works with RichTextContentLocal instead of just newText: String
 */
fun createChatMessageMigration3to4(): ChatMessageContentMigration<*, *> = ChatMessageContentMigration(
    versionFrom = 37,
    versionTo = 38,
    oldSerializer = ContentV3.serializer(),
    newSerializer = ContentV4.serializer(),
    mapper = { old ->
        when (old) {
            is ContentV3.Text -> ContentV4.Text(old.text)
            is ContentV3.Token -> ContentV4.Token(old.token, old.platform)
            ContentV3.ContactAdded -> ContentV4.ContactAdded
            is ContentV3.Reacted -> ContentV4.Reacted(old.messageId, old.content)
            is ContentV3.ReactionRemoved -> ContentV4.ReactionRemoved(old.messageId, old.content)
            is ContentV3.RichText -> ContentV4.RichText(old.content)
            is ContentV3.Unsupported -> ContentV4.Unsupported(old.rawContent)
            ContentV3.LeftChat -> ContentV4.LeftChat
            is ContentV3.Edited -> ContentV4.Edited(
                old.messageId, RichTextContentLocal(
                    text = old.newText,
                    attachments = emptyList()
                )
            )

            is ContentV3.DataChannelOffer -> ContentV4.DataChannelOffer(old.sdp, old.purpose)
            is ContentV3.DataChannelAnswer -> ContentV4.DataChannelAnswer(old.offerMessageId, old.sdp)
            is ContentV3.DataChannelIceCandidate -> ContentV4.DataChannelIceCandidate(old.offerMessageId, old.sdp)
            is ContentV3.ChatAccepted -> ContentV4.ChatAccepted(old.requestId)
            is ContentV3.ChatRequest -> ContentV4.ChatRequest(
                welcome = old.welcome?.let { RichTextContentLocal(text = it.text, attachments = null) }
            )

            is ContentV3.CoinagePayment -> ContentV4.CoinagePayment(
                totalValue = old.totalValue,
                coinKeys = old.coinKeys,
                status = old.status
            )

            is ContentV3.DataChannelClosed -> ContentV4.DataChannelClosed(old.offerMessageId)
            is ContentV3.Custom -> ContentV4.Custom(old.rendererId, old.rawContent)
        }
    }
)
