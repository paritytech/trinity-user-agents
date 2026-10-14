package io.paritytech.polkadotapp.feature_chats_impl.data.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.ChatPushContent
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.StrippedChatPushContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatement
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatementContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationChatMessage
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationChatMessageContentV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationMessagePayload
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.StrippedChatMessageContentV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.VersionedChatMessage
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.VersionedNotificationChatMessageContent
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage

internal enum class NotificationPayloadMode {
    FULL, STRIPPED
}

internal fun ChatMessage.toNotificationPayload(mode: NotificationPayloadMode): Result<NotificationMessagePayload> {
    return toWireContent()
        .flatMap { content ->
            when (mode) {
                NotificationPayloadMode.FULL -> Result.success(NotificationChatMessageContentV1.Full(content))
                NotificationPayloadMode.STRIPPED -> content.toStripped().map(NotificationChatMessageContentV1::Stripped)
            }
        }
        .map { content ->
            NotificationMessagePayload(
                id = id,
                timestamp = timestamp.toULong(),
                message = NotificationChatMessage(VersionedNotificationChatMessageContent.V1(content)),
            )
        }
}

private fun ChatMessageStatementContent.toStripped(): Result<StrippedChatMessageContentV1> {
    return runCatching {
        when (this) {
            is ChatMessageStatementContent.Text -> StrippedChatMessageContentV1.Text(text)
            ChatMessageStatementContent.ContactAdded -> StrippedChatMessageContentV1.ContactAdded
            is ChatMessageStatementContent.Reacted -> StrippedChatMessageContentV1.Reacted(messageId, content)
            is ChatMessageStatementContent.Reply -> StrippedChatMessageContentV1.Reply(messageId, ownContent)
            is ChatMessageStatementContent.DataChannelOffer -> StrippedChatMessageContentV1.DataChannelOffer(sdp, purpose)
            is ChatMessageStatementContent.ChatAccepted -> StrippedChatMessageContentV1.ChatAccepted(requestId)
            is ChatMessageStatementContent.RichText -> StrippedChatMessageContentV1.RichText(content)
            is ChatMessageStatementContent.CoinagePayment -> StrippedChatMessageContentV1.CoinagePayment(totalValue)
            is ChatMessageStatementContent.DeviceChatAccepted -> StrippedChatMessageContentV1.DeviceChatAccepted(requestId, device)

            is ChatMessageStatementContent.Token,
            is ChatMessageStatementContent.ReactionRemoved,
            is ChatMessageStatementContent.DataChannelAnswer,
            is ChatMessageStatementContent.DataChannelIceCandidate,
            is ChatMessageStatementContent.DataChannelClosed,
            is ChatMessageStatementContent.Edited,
            ChatMessageStatementContent.LeftChat,
            is ChatMessageStatementContent.DeviceAdded,
            is ChatMessageStatementContent.DeviceRemoved,
            is ChatMessageStatementContent.CompactionCommit -> error("${this::class.simpleName} is not displayable as push")
        }
    }
}

internal fun EncodedMessage.toChatPushContent(
    authorAccountId: AccountId,
    contactAccountId: AccountId,
    messageStatus: ChatMessage.Status,
): Result<ChatPushContent> {
    return runCatching { BinaryScale.decodeFromByteArray<NotificationMessagePayload>(this) }
        .map { it.toChatPushContent(authorAccountId, contactAccountId, messageStatus) }
        .flatRecover { toUnsupportedStrippedPushContent() }
}

private fun NotificationMessagePayload.toChatPushContent(
    authorAccountId: AccountId,
    contactAccountId: AccountId,
    messageStatus: ChatMessage.Status,
): ChatPushContent {
    return when (val versioned = message.versioned) {
        is VersionedNotificationChatMessageContent.V1 -> when (val content = versioned.content) {
            is NotificationChatMessageContentV1.Full -> ChatPushContent.Full(
                ChatMessageStatement(id, timestamp, VersionedChatMessage.V1(ChatMessageV1(content.content)))
                    .toChatMessage(authorAccountId, contactAccountId, messageStatus)
            )

            is NotificationChatMessageContentV1.Stripped -> ChatPushContent.Stripped(
                messageId = id,
                timestamp = timestamp.toLong(),
                content = content.content.toStrippedChatPushContent()
            )
        }
    }
}

private fun StrippedChatMessageContentV1.toStrippedChatPushContent(): StrippedChatPushContent {
    return when (this) {
        is StrippedChatMessageContentV1.CoinagePayment -> StrippedChatPushContent.CoinagePayment(totalBalance)
        is StrippedChatMessageContentV1.Text -> regular(ChatMessageStatementContent.Text(text))
        StrippedChatMessageContentV1.ContactAdded -> regular(ChatMessageStatementContent.ContactAdded)
        is StrippedChatMessageContentV1.Reacted -> regular(ChatMessageStatementContent.Reacted(messageId, content))
        is StrippedChatMessageContentV1.Reply -> regular(ChatMessageStatementContent.Reply(messageId, ownContent))
        is StrippedChatMessageContentV1.DataChannelOffer -> regular(ChatMessageStatementContent.DataChannelOffer(sdp, purpose))
        is StrippedChatMessageContentV1.ChatAccepted -> regular(ChatMessageStatementContent.ChatAccepted(requestId))
        is StrippedChatMessageContentV1.RichText -> regular(ChatMessageStatementContent.RichText(content))
        is StrippedChatMessageContentV1.DeviceChatAccepted -> regular(ChatMessageStatementContent.DeviceChatAccepted(requestId, device))
    }
}

private fun regular(content: ChatMessageStatementContent): StrippedChatPushContent {
    return StrippedChatPushContent.Regular(content.toChatMessageContent())
}

private fun EncodedMessage.toUnsupportedStrippedPushContent(): Result<ChatPushContent> {
    return decodeAlwaysDecodableChatMessagePart().map {
        ChatPushContent.Stripped(
            messageId = it.id,
            timestamp = it.timestamp.toLong(),
            content = StrippedChatPushContent.Regular(ChatMessage.Content.Unsupported(this))
        )
    }
}
