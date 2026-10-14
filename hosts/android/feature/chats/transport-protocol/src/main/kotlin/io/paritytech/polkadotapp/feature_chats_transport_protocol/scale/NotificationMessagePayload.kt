package io.paritytech.polkadotapp.feature_chats_transport_protocol.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.chains.network.binding.Balance
import kotlinx.serialization.Serializable

@Serializable
class NotificationMessagePayload(
    val id: String,
    val timestamp: ULong,
    val message: NotificationChatMessage,
)

@Serializable
class NotificationChatMessage(
    val versioned: VersionedNotificationChatMessageContent,
)

@Serializable
sealed interface VersionedNotificationChatMessageContent {
    @Serializable
    @EnumIndex(0)
    class V1(val content: NotificationChatMessageContentV1) : VersionedNotificationChatMessageContent
}

@Serializable
sealed interface NotificationChatMessageContentV1 {
    @Serializable
    @EnumIndex(0)
    class Stripped(val content: StrippedChatMessageContentV1) : NotificationChatMessageContentV1

    @Serializable
    @EnumIndex(1)
    class Full(val content: ChatMessageStatementContent) : NotificationChatMessageContentV1
}

// Indices mirror ChatMessageStatementContent; only variants that can be pushed are present
@Serializable
sealed interface StrippedChatMessageContentV1 {
    @Serializable
    @EnumIndex(0)
    class Text(val text: String) : StrippedChatMessageContentV1

    @Serializable
    @EnumIndex(3)
    object ContactAdded : StrippedChatMessageContentV1

    @Serializable
    @EnumIndex(4)
    class Reacted(
        val messageId: String,
        val content: ChatMessageReactionStatementContent,
    ) : StrippedChatMessageContentV1

    @Serializable
    @EnumIndex(7)
    class Reply(
        val messageId: String,
        val ownContent: RichTextContent,
    ) : StrippedChatMessageContentV1

    @Serializable
    @EnumIndex(8)
    class DataChannelOffer(
        val sdp: ByteArray,
        val purpose: DataChannelPurpose,
    ) : StrippedChatMessageContentV1

    @Serializable
    @EnumIndex(14)
    class ChatAccepted(
        val requestId: String,
    ) : StrippedChatMessageContentV1

    @Serializable
    @EnumIndex(15)
    class RichText(val content: RichTextContent) : StrippedChatMessageContentV1

    @Serializable
    @EnumIndex(16)
    class CoinagePayment(
        val totalBalance: Balance,
    ) : StrippedChatMessageContentV1

    @Serializable
    @EnumIndex(20)
    class DeviceChatAccepted(
        val requestId: String,
        val device: DeviceInfoScale,
    ) : StrippedChatMessageContentV1
}
