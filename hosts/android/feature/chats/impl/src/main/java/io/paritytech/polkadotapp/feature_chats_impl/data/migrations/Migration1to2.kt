package io.paritytech.polkadotapp.feature_chats_impl.data.migrations

import io.paritytech.polkadotapp.database.migrations.ChatMessageContentMigration
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.ContentV1
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.ContentV2
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.LegacyCoinagePaymentStatusLocal

/**
 * Content schema migration V1 → V2.
 * Adds [LegacyCoinagePaymentStatusLocal] to CoinagePayment content.
 * Old messages without status are migrated to [LegacyCoinagePaymentStatusLocal.TRANSFERRED].
 */
fun createChatMessageMigration1to2(): ChatMessageContentMigration<*, *> = ChatMessageContentMigration(
    versionFrom = 25,
    versionTo = 26,
    oldSerializer = ContentV1.serializer(),
    newSerializer = ContentV2.serializer(),
    mapper = { old ->
        when (old) {
            is ContentV1.CoinagePayment -> ContentV2.CoinagePayment(
                totalValue = old.totalValue,
                coinKeys = old.coinKeys,
                status = LegacyCoinagePaymentStatusLocal.TRANSFERRED,
            )
            is ContentV1.Text -> ContentV2.Text(old.text)
            is ContentV1.Token -> ContentV2.Token(old.token, old.platform)
            ContentV1.ContactAdded -> ContentV2.ContactAdded
            is ContentV1.Reacted -> ContentV2.Reacted(old.messageId, old.content)
            is ContentV1.ReactionRemoved -> ContentV2.ReactionRemoved(old.messageId, old.content)
            is ContentV1.Media -> ContentV2.Media(old.url, old.text, old.type, old.aspectRatio)
            is ContentV1.Unsupported -> ContentV2.Unsupported(old.rawContent)
            is ContentV1.File -> ContentV2.File(old.url, old.fileName, old.mimeType, old.sizeBytes, old.text)
            ContentV1.LeftChat -> ContentV2.LeftChat
            is ContentV1.Edited -> ContentV2.Edited(old.messageId, old.newText)
            is ContentV1.DataChannelOffer -> ContentV2.DataChannelOffer(old.sdp, old.purpose)
            is ContentV1.DataChannelAnswer -> ContentV2.DataChannelAnswer(old.offerMessageId, old.sdp)
            is ContentV1.DataChannelIceCandidate -> ContentV2.DataChannelIceCandidate(old.offerMessageId, old.sdp)
            is ContentV1.ChatAccepted -> ContentV2.ChatAccepted(old.requestId)
            is ContentV1.ChatRequest -> ContentV2.ChatRequest(old.welcome)
            is ContentV1.DataChannelClosed -> ContentV2.DataChannelClosed(old.offerMessageId)
            is ContentV1.Custom -> ContentV2.Custom(old.rendererId, old.rawContent)
        }
    }
)
