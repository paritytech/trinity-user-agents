package io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale

import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo
import kotlinx.serialization.ExperimentalSerializationApi

@OptIn(ExperimentalSerializationApi::class)
fun ChatMessageContentLocal.toDomain(
    customContentDecoder: CustomContentDecoder,
): ChatMessage.Content {
    return when (this) {
        is ChatMessageContentLocal.ContactAdded -> ChatMessage.Content.ContactAdded
        is ChatMessageContentLocal.CoinagePayment -> ChatMessage.Content.CoinagePayment(totalValue, coinKeys, status.toDomain())
        is ChatMessageContentLocal.Text -> ChatMessage.Content.Text(text)
        is ChatMessageContentLocal.Token -> ChatMessage.Content.Token(token.toDataByteArray(), platform.toOS())
        is ChatMessageContentLocal.RichText -> content.toRichTextDomain()
        is ChatMessageContentLocal.Reacted -> ChatMessage.Content.Reacted(messageId, ChatMessageReactionContent(content.emoji))
        is ChatMessageContentLocal.ReactionRemoved -> ChatMessage.Content.ReactionRemoved(messageId, ChatMessageReactionContent(content.emoji))
        is ChatMessageContentLocal.Unsupported -> ChatMessage.Content.Unsupported(rawContent)
        is ChatMessageContentLocal.LeftChat -> ChatMessage.Content.LeftChat
        is ChatMessageContentLocal.Edited -> ChatMessage.Content.Edited(messageId, richTextContent.toRichTextDomain())
        is ChatMessageContentLocal.ChatAccepted -> ChatMessage.Content.ChatAccepted(requestId)
        is ChatMessageContentLocal.DeviceChatAccepted -> ChatMessage.Content.DeviceChatAccepted(
            requestId = requestId,
            device = device.toDomain(),
        )
        is ChatMessageContentLocal.ChatRequest -> ChatMessage.Content.ChatRequest(welcome?.toRichTextDomain())
        is ChatMessageContentLocal.DataChannelAnswer -> ChatMessage.Content.DataChannelAnswer(offerMessageId, sdp)
        is ChatMessageContentLocal.DataChannelIceCandidate -> ChatMessage.Content.DataChannelIceCandidate(offerMessageId, sdp)
        is ChatMessageContentLocal.DataChannelOffer -> ChatMessage.Content.DataChannelOffer(sdp, purpose.toDomain())
        is ChatMessageContentLocal.DataChannelClosed -> ChatMessage.Content.DataChannelClosed(offerMessageId)
        is ChatMessageContentLocal.DeviceAdded -> ChatMessage.Content.DeviceAdded(statementAccountId, encryptionPublicKey.requireX25519PublicKey())
        is ChatMessageContentLocal.DeviceRemoved -> ChatMessage.Content.DeviceRemoved(statementAccountId)
        is ChatMessageContentLocal.CompactionCommit -> ChatMessage.Content.CompactionCommit(
            claimIdentifier = claimIdentifier.toDataByteArray(),
            claimTicket = HopTicket.fromRaw(claimTicket),
            nodeUrl = nodeUrl
        )
        is ChatMessageContentLocal.CompactionUnavailable -> ChatMessage.Content.CompactionUnavailable

        is ChatMessageContentLocal.Custom -> {
            val decoded = rawContent?.let { customContentDecoder.decode(rendererId, rawContent) }
                ?: Result.failure(IllegalStateException("Content was not found"))

            ChatMessage.Content.Custom(rendererId, decoded)
        }
    }
}

fun ChatMessage.Content.toData(
    customContentDecoder: CustomContentDecoder,
): ChatMessageContentLocal {
    return when (this) {
        ChatMessage.Content.ContactAdded -> ChatMessageContentLocal.ContactAdded
        is ChatMessage.Content.CoinagePayment -> ChatMessageContentLocal.CoinagePayment(totalValue, coinKeys, status.toLocal())
        is ChatMessage.Content.Text -> ChatMessageContentLocal.Text(text)
        is ChatMessage.Content.Token -> ChatMessageContentLocal.Token(token.value, operatingSystem.toTokenPlatformLocal())
        is ChatMessage.Content.RichText -> ChatMessageContentLocal.RichText(toRichTextLocal())
        is ChatMessage.Content.Reacted -> ChatMessageContentLocal.Reacted(messageId, ChatMessageReactionContentLocal(content.emoji))
        is ChatMessage.Content.ReactionRemoved -> ChatMessageContentLocal.ReactionRemoved(messageId, ChatMessageReactionContentLocal(content.emoji))
        is ChatMessage.Content.Unsupported -> ChatMessageContentLocal.Unsupported(rawContent)
        ChatMessage.Content.LeftChat -> ChatMessageContentLocal.LeftChat
        is ChatMessage.Content.Edited -> ChatMessageContentLocal.Edited(messageId, content.toRichTextLocal())
        is ChatMessage.Content.ChatAccepted -> ChatMessageContentLocal.ChatAccepted(requestId)
        is ChatMessage.Content.DeviceChatAccepted -> ChatMessageContentLocal.DeviceChatAccepted(
            requestId = requestId,
            device = device.toLocal(),
        )
        is ChatMessage.Content.ChatRequest -> ChatMessageContentLocal.ChatRequest(welcome?.toRichTextLocal())
        is ChatMessage.Content.Custom<*> -> {
            val encoded = content.getOrNull()
                ?.let { customContentDecoder.encode(rendererId, it) }
                ?.getOrNull()
            ChatMessageContentLocal.Custom(rendererId, encoded)
        }

        is ChatMessage.Content.DataChannelAnswer -> ChatMessageContentLocal.DataChannelAnswer(offerMessageId, sdp)
        is ChatMessage.Content.DataChannelIceCandidate -> ChatMessageContentLocal.DataChannelIceCandidate(offerMessageId, sdp)
        is ChatMessage.Content.DataChannelOffer -> ChatMessageContentLocal.DataChannelOffer(sdp, purpose.toDataChannelPurposeLocal())
        is ChatMessage.Content.DataChannelClosed -> ChatMessageContentLocal.DataChannelClosed(offerMessageId)
        is ChatMessage.Content.DeviceAdded -> ChatMessageContentLocal.DeviceAdded(statementAccountId, encryptionPublicKey.bytes)
        is ChatMessage.Content.DeviceRemoved -> ChatMessageContentLocal.DeviceRemoved(statementAccountId)
        is ChatMessage.Content.CompactionCommit -> ChatMessageContentLocal.CompactionCommit(
            claimIdentifier = claimIdentifier.value,
            claimTicket = claimTicket.bytes,
            nodeUrl = nodeUrl
        )
        ChatMessage.Content.CompactionUnavailable -> ChatMessageContentLocal.CompactionUnavailable
    }
}

private fun RichTextContentLocal.toRichTextDomain(): ChatMessage.Content.RichText {
    return ChatMessage.Content.RichText(
        text = text,
        attachments = attachments?.map { it.toDomain() }.orEmpty()
    )
}

private fun ChatMessage.Content.RichText.toRichTextLocal(): RichTextContentLocal {
    return RichTextContentLocal(
        text = text,
        attachments = attachments.ifEmpty { null }?.map { it.toLocal() }
    )
}

private fun AttachmentLocal.toDomain(): Attachment {
    return when (this) {
        is AttachmentLocal.Hosted -> Attachment.Hosted(
            uri = uri?.toUri(),
            identifier = identifier.toDataByteArray(),
            ticket = HopTicket.fromRaw(claimTicket),
            nodeUrl = nodeUrl,
            meta = meta.toDomain()
        )

        is AttachmentLocal.Embedded -> Attachment.Embedded(
            uri = uri.toUri(),
            meta = meta.toDomain()
        )
    }
}

private fun AttachmentMetaLocal.toDomain(): Attachment.Meta {
    return when (this) {
        is AttachmentMetaLocal.Image -> Attachment.Meta.Image(
            width = width,
            height = height,
            blurHash = blurHash,
            mimeType = mimeType,
            size = sizeBytes.bytes
        )
        is AttachmentMetaLocal.Video -> Attachment.Meta.Video(
            duration = duration,
            blurHash = blurHash,
            mimeType = mimeType,
            size = sizeBytes.bytes
        )
        is AttachmentMetaLocal.General -> Attachment.Meta.General(fileName, mimeType, sizeBytes.bytes)
    }
}

private fun Attachment.toLocal(): AttachmentLocal {
    return when (this) {
        is Attachment.Hosted -> AttachmentLocal.Hosted(
            uri = uri?.toString(),
            identifier = identifier.value,
            claimTicket = ticket.bytes,
            nodeUrl = nodeUrl,
            meta = meta.toLocal()
        )

        is Attachment.Embedded -> AttachmentLocal.Embedded(
            uri = uri.toString(),
            meta = meta.toLocal()
        )
    }
}

private fun Attachment.Meta.toLocal(): AttachmentMetaLocal {
    return when (this) {
        is Attachment.Meta.Image -> AttachmentMetaLocal.Image(
            width = width,
            height = height,
            mimeType = mimeType,
            sizeBytes = size.inWholeBytes,
            blurHash = blurHash
        )
        is Attachment.Meta.Video -> AttachmentMetaLocal.Video(
            duration = duration,
            mimeType = mimeType,
            sizeBytes = size.inWholeBytes,
            blurHash = blurHash
        )
        is Attachment.Meta.General -> AttachmentMetaLocal.General(fileName, mimeType, size.inWholeBytes)
    }
}

private fun TokenPlatformLocal.toOS(): OperatingSystem = when (this) {
    TokenPlatformLocal.ANDROID -> OperatingSystem.ANDROID
    TokenPlatformLocal.IOS -> OperatingSystem.IOS
}

private fun OperatingSystem.toTokenPlatformLocal(): TokenPlatformLocal = when (this) {
    OperatingSystem.IOS -> TokenPlatformLocal.IOS
    OperatingSystem.ANDROID,
    OperatingSystem.UNKNOWN -> TokenPlatformLocal.ANDROID
}

private fun ChatMessage.Content.DataChannelOffer.Purpose.toDataChannelPurposeLocal(): OfferPurpose = when (this) {
    ChatMessage.Content.DataChannelOffer.Purpose.AUDIO_CALL -> OfferPurpose.AUDIO_CALL
    ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL -> OfferPurpose.VIDEO_CALL
}

private fun OfferPurpose.toDomain(): ChatMessage.Content.DataChannelOffer.Purpose = when (this) {
    OfferPurpose.AUDIO_CALL -> ChatMessage.Content.DataChannelOffer.Purpose.AUDIO_CALL
    OfferPurpose.VIDEO_CALL -> ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL
}

private fun CoinagePaymentStatusLocal.toDomain(): ChatMessage.Content.CoinagePayment.Status = when (this) {
    is CoinagePaymentStatusLocal.Detecting -> ChatMessage.Content.CoinagePayment.Status.Detecting
    is CoinagePaymentStatusLocal.Detected -> ChatMessage.Content.CoinagePayment.Status.Detected(detected)
    is CoinagePaymentStatusLocal.PartiallyClaimed -> ChatMessage.Content.CoinagePayment.Status.PartiallyClaimed(claimed)
    is CoinagePaymentStatusLocal.Transferred -> ChatMessage.Content.CoinagePayment.Status.Transferred(transferred)
    is CoinagePaymentStatusLocal.FailedDetection -> ChatMessage.Content.CoinagePayment.Status.FailedDetection
    is CoinagePaymentStatusLocal.FailedTransfer -> ChatMessage.Content.CoinagePayment.Status.FailedTransfer
}

private fun DeviceInfo.toLocal(): DeviceInfoLocal {
    return DeviceInfoLocal(
        statementAccountId = statementAccountId,
        encryptionPublicKey = encryptionPublicKey.bytes,
    )
}

private fun DeviceInfoLocal.toDomain(): DeviceInfo {
    return DeviceInfo(
        statementAccountId = statementAccountId,
        encryptionPublicKey = encryptionPublicKey.requireX25519PublicKey(),
    )
}

private fun ChatMessage.Content.CoinagePayment.Status.toLocal(): CoinagePaymentStatusLocal = when (this) {
    is ChatMessage.Content.CoinagePayment.Status.Detecting -> CoinagePaymentStatusLocal.Detecting
    is ChatMessage.Content.CoinagePayment.Status.Detected -> CoinagePaymentStatusLocal.Detected(amount)
    is ChatMessage.Content.CoinagePayment.Status.PartiallyClaimed -> CoinagePaymentStatusLocal.PartiallyClaimed(claimed)
    is ChatMessage.Content.CoinagePayment.Status.Transferred -> CoinagePaymentStatusLocal.Transferred(amount)
    is ChatMessage.Content.CoinagePayment.Status.FailedDetection -> CoinagePaymentStatusLocal.FailedDetection
    is ChatMessage.Content.CoinagePayment.Status.FailedTransfer -> CoinagePaymentStatusLocal.FailedTransfer
}
