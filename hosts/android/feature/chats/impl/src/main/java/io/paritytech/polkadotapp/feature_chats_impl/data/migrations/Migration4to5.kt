package io.paritytech.polkadotapp.feature_chats_impl.data.migrations

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.database.migrations.ChatMessageContentMigration
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.ContentV4
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.LegacyCoinagePaymentStatusLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.CoinagePaymentStatusLocal

/**
 * Content schema migration V4 → V5.
 * CoinagePayment.status becomes a sealed type whose Detected and Transferred variants carry the
 * detected Balance. Pre-V5 rows already hold the detected value in totalValue (it was overwritten on
 * detection), so DETECTED/TRANSFERRED are migrated carrying detected = totalValue — the only Balance available.
 */
fun createChatMessageMigration4to5(): ChatMessageContentMigration<*, *> = ChatMessageContentMigration(
    versionFrom = 44,
    versionTo = 45,
    oldSerializer = ContentV4.serializer(),
    newSerializer = ChatMessageContentLocal.serializer(),
    mapper = { old ->
        when (old) {
            is ContentV4.Text -> ChatMessageContentLocal.Text(old.text)
            is ContentV4.Token -> ChatMessageContentLocal.Token(old.token, old.platform)
            ContentV4.ContactAdded -> ChatMessageContentLocal.ContactAdded
            is ContentV4.Reacted -> ChatMessageContentLocal.Reacted(old.messageId, old.content)
            is ContentV4.ReactionRemoved -> ChatMessageContentLocal.ReactionRemoved(old.messageId, old.content)
            is ContentV4.RichText -> ChatMessageContentLocal.RichText(old.content)
            is ContentV4.Unsupported -> ChatMessageContentLocal.Unsupported(old.rawContent)
            ContentV4.LeftChat -> ChatMessageContentLocal.LeftChat
            is ContentV4.Edited -> ChatMessageContentLocal.Edited(old.messageId, old.richTextContent)
            is ContentV4.DataChannelOffer -> ChatMessageContentLocal.DataChannelOffer(old.sdp, old.purpose)
            is ContentV4.DataChannelAnswer -> ChatMessageContentLocal.DataChannelAnswer(old.offerMessageId, old.sdp)
            is ContentV4.DataChannelIceCandidate -> ChatMessageContentLocal.DataChannelIceCandidate(old.offerMessageId, old.sdp)
            is ContentV4.ChatAccepted -> ChatMessageContentLocal.ChatAccepted(old.requestId)
            is ContentV4.ChatRequest -> ChatMessageContentLocal.ChatRequest(old.welcome)
            is ContentV4.DeviceAdded -> ChatMessageContentLocal.DeviceAdded(old.statementAccountId, old.encryptionPublicKey)
            is ContentV4.DeviceRemoved -> ChatMessageContentLocal.DeviceRemoved(old.statementAccountId)
            is ContentV4.DeviceChatAccepted -> ChatMessageContentLocal.DeviceChatAccepted(old.requestId, old.device)

            is ContentV4.CoinagePayment -> ChatMessageContentLocal.CoinagePayment(
                totalValue = old.totalValue,
                coinKeys = old.coinKeys,
                status = old.status.toSealed(detectedFallback = old.totalValue)
            )

            is ContentV4.DataChannelClosed -> ChatMessageContentLocal.DataChannelClosed(old.offerMessageId)
            is ContentV4.Custom -> ChatMessageContentLocal.Custom(old.rendererId, old.rawContent)
        }
    }
)

internal fun LegacyCoinagePaymentStatusLocal.toSealed(
    detectedFallback: Balance
): CoinagePaymentStatusLocal = when (this) {
    LegacyCoinagePaymentStatusLocal.DETECTING -> CoinagePaymentStatusLocal.Detecting
    LegacyCoinagePaymentStatusLocal.DETECTED -> CoinagePaymentStatusLocal.Detected(detectedFallback)
    LegacyCoinagePaymentStatusLocal.TRANSFERRED -> CoinagePaymentStatusLocal.Transferred(detectedFallback)
    LegacyCoinagePaymentStatusLocal.FAILED_DETECTION -> CoinagePaymentStatusLocal.FailedDetection
    LegacyCoinagePaymentStatusLocal.FAILED_TRANSFER -> CoinagePaymentStatusLocal.FailedTransfer
}
