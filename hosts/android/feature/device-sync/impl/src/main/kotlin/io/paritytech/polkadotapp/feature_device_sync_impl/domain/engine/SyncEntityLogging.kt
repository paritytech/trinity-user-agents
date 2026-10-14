package io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatement
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatementContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.LocalMessageScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.LocalStatusScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.VersionedChatMessage

internal fun LocalMessageScale.logMessage(): String =
    "${remote.contentOrNull().logContent()} " +
        "[id=${remote.id}, peer=${peerId.shortHex()}, status=${status.logDirection()}, order=$order]"

internal fun ChatMessageStatement.contentOrNull(): ChatMessageStatementContent =
    when (val versioned = versioned) {
        is VersionedChatMessage.V1 -> versioned.message.content
    }

internal fun ChatMessageStatementContent.logContent(): String = when (this) {
    is ChatMessageStatementContent.Text -> "Text(\"${text.take(60)}\")"
    is ChatMessageStatementContent.RichText ->
        "RichText(text=\"${content.text?.take(60)}\", attachments=${content.attachments?.size ?: 0})"

    is ChatMessageStatementContent.Reply -> "Reply($messageId, \"${ownContent.text?.take(60)}\")"
    is ChatMessageStatementContent.Edited -> "Edited($messageId, \"${newContent.text?.take(60)}\")"
    is ChatMessageStatementContent.Reacted -> "Reacted($messageId, ${content.emoji})"
    is ChatMessageStatementContent.ReactionRemoved -> "ReactionRemoved($messageId, ${content.emoji})"
    is ChatMessageStatementContent.Token -> "Token(${content.platform})"
    is ChatMessageStatementContent.ContactAdded -> "ContactAdded"
    is ChatMessageStatementContent.LeftChat -> "LeftChat"
    is ChatMessageStatementContent.ChatAccepted -> "ChatAccepted($requestId)"
    is ChatMessageStatementContent.DeviceChatAccepted -> "DeviceChatAccepted($requestId)"
    is ChatMessageStatementContent.DeviceAdded -> "DeviceAdded(${statementAccountId.value.shortHex()})"
    is ChatMessageStatementContent.DeviceRemoved -> "DeviceRemoved(${statementAccountId.value.shortHex()})"
    is ChatMessageStatementContent.CoinagePayment -> "CoinagePayment(total=$totalValue)"
    is ChatMessageStatementContent.DataChannelOffer -> "DataChannelOffer(purpose=$purpose)"
    is ChatMessageStatementContent.DataChannelAnswer -> "DataChannelAnswer(offer=$offerMessageId)"
    is ChatMessageStatementContent.DataChannelIceCandidate -> "DataChannelIceCandidate(offer=$offerMessageId)"
    is ChatMessageStatementContent.DataChannelClosed -> "DataChannelClosed(offer=$offerMessageId)"
    is ChatMessageStatementContent.CompactionCommit -> "CompactionCommit(${claimIdentifier.shortHex()})"
}

internal fun LocalStatusScale.logDirection(): String = when (this) {
    is LocalStatusScale.Outgoing -> "Outgoing.$status"
    is LocalStatusScale.Incoming -> "Incoming.$status"
}

private fun ByteArray.shortHex(): String = toHexString(withPrefix = false).take(7)
