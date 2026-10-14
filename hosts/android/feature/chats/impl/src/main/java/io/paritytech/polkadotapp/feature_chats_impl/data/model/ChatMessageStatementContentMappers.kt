package io.paritytech.polkadotapp.feature_chats_impl.data.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.scale.toDomain
import io.paritytech.polkadotapp.common.domain.model.scale.toScale
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.encodeToByteArrayCatching
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.AlwaysDecodableChatMessagePart
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageReactionStatementContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatement
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatementContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.DataChannelPurpose
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.DeviceInfoScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.FileMeta
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.FileVariant
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.GeneralFileMeta
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ImageFileMeta
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.MediaThumbnail
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NodeEndpoint
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.P2PMixnetFile
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.RichTextContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenPlatform
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.VersionedChatMessage
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.VideoFileMeta
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage

fun ChatMessage.toEncodedMessage(): Result<EncodedMessage> {
    return toStatementStoreMessage()
        .flatMap { BinaryScale.encodeToByteArrayCatching(it) }
}

private fun ChatMessage.toStatementStoreMessage(): Result<ChatMessageStatement> {
    return toWireContent()
        .map {
            val message = ChatMessageV1(it)
            val versioned = VersionedChatMessage.V1(message)
            ChatMessageStatement(id, timestamp.toULong(), versioned)
        }
}

internal fun ChatMessage.toWireContent(): Result<ChatMessageStatementContent> {
    return content.toStatementStoreContent()
        .map { it.wrapInReplyOrSelf(replyToMessageId) }
}

fun EncodedMessage.decodeChatMessageStatement(): Result<ChatMessageStatement> {
    return runCatching { BinaryScale.decodeFromByteArray<ChatMessageStatement>(this) }
}

fun EncodedMessage.decodeAlwaysDecodableChatMessagePart(): Result<AlwaysDecodableChatMessagePart> {
    return runCatching { BinaryScale.decodeFromByteArray<AlwaysDecodableChatMessagePart>(this) }
        .logFailure("Failed to parse AlwaysDecodableChatMessagePart - this should never happen")
}

fun EncodedMessage.toChatMessage(
    authorAccountId: AccountId,
    contactAccountId: AccountId,
    messageStatus: ChatMessage.Status,
): Result<ChatMessage> {
    return decodeChatMessageStatement()
        .map { it.toChatMessage(authorAccountId, contactAccountId, messageStatus) }
}

fun EncodedMessage.toChatMessageOrUnsupported(
    authorAccountId: AccountId,
    contactAccountId: AccountId,
    messageStatus: ChatMessage.Status,
): Result<ChatMessage> {
    return decodeChatMessageStatement()
        .map { it.toChatMessage(authorAccountId, contactAccountId, messageStatus) }
        .flatRecover { toUnsupportedChatMessage(authorAccountId, contactAccountId, messageStatus) }
}

fun EncodedMessage.toUnsupportedChatMessage(
    authorAccountId: AccountId,
    contactAccountId: AccountId,
    messageStatus: ChatMessage.Status,
): Result<ChatMessage> {
    return decodeAlwaysDecodableChatMessagePart().map { alwaysDecodableChatMessagePart ->
        ChatMessage(
            id = alwaysDecodableChatMessagePart.id,
            timestamp = alwaysDecodableChatMessagePart.timestamp.toLong(),
            origin = ChatMessageOrigin.fromContactChat(authorAccountId, contactAccountId),
            content = ChatMessage.Content.Unsupported(this),
            status = messageStatus,
            chatId = ChatId.fromContact(contactAccountId)
        )
    }
}

private fun ChatMessage.Content.toStatementStoreContent(): Result<ChatMessageStatementContent> {
    return runCatching {
        when (this) {
            ChatMessage.Content.ContactAdded -> ChatMessageStatementContent.ContactAdded

            is ChatMessage.Content.CoinagePayment -> ChatMessageStatementContent.CoinagePayment(totalValue, coinKeys)

            is ChatMessage.Content.Text -> ChatMessageStatementContent.Text(text)

            is ChatMessage.Content.Token -> ChatMessageStatementContent.Token(
                TokenContent(token.value, if (isVoIP) TokenPlatform.IOS_VOIP else operatingSystem.toTokenPlatform())
            )

            is ChatMessage.Content.RichText -> ChatMessageStatementContent.RichText(
                RichTextContent(text = text, attachments = attachments.toData())
            )

            is ChatMessage.Content.Reacted -> ChatMessageStatementContent.Reacted(
                messageId,
                content = ChatMessageReactionStatementContent(content.emoji)
            )

            is ChatMessage.Content.ReactionRemoved -> ChatMessageStatementContent.ReactionRemoved(
                messageId,
                content = ChatMessageReactionStatementContent(content.emoji)
            )

            is ChatMessage.Content.LeftChat -> ChatMessageStatementContent.LeftChat

            is ChatMessage.Content.Edited -> ChatMessageStatementContent.Edited(
                messageId,
                newContent = RichTextContent(text = content.text, attachments = content.attachments.toData())
            )

            is ChatMessage.Content.ChatAccepted -> ChatMessageStatementContent.ChatAccepted(requestId)
            is ChatMessage.Content.DeviceChatAccepted -> ChatMessageStatementContent.DeviceChatAccepted(
                requestId = requestId,
                device = device.toScale(),
            )
            is ChatMessage.Content.ChatRequest -> error("ChatRequest is local-only and cannot be sent over network")
            is ChatMessage.Content.Unsupported -> error("Cannot send unsupported content over network")
            is ChatMessage.Content.Custom<*> -> error("Cannot send custom content over network")
            is ChatMessage.Content.DataChannelAnswer -> ChatMessageStatementContent.DataChannelAnswer(offerMessageId, sdp)
            is ChatMessage.Content.DataChannelIceCandidate -> ChatMessageStatementContent.DataChannelIceCandidate(offerMessageId, sdp)
            is ChatMessage.Content.DataChannelOffer -> ChatMessageStatementContent.DataChannelOffer(sdp, purpose.toDataChannelPurpose())
            is ChatMessage.Content.DataChannelClosed -> ChatMessageStatementContent.DataChannelClosed(offerMessageId)
            is ChatMessage.Content.DeviceAdded -> ChatMessageStatementContent.DeviceAdded(statementAccountId, encryptionPublicKey.bytes)
            is ChatMessage.Content.DeviceRemoved -> ChatMessageStatementContent.DeviceRemoved(statementAccountId)
            is ChatMessage.Content.CompactionCommit -> ChatMessageStatementContent.CompactionCommit(
                claimIdentifier = claimIdentifier.value,
                claimTicket = claimTicket.bytes,
                node = NodeEndpoint.WssUrl(nodeUrl)
            )
            is ChatMessage.Content.CompactionUnavailable -> error("CompactionUnavailable is local-only and cannot be sent over network")
        }
    }
}

private fun List<Attachment>.toData() = map {
    require(it is Attachment.Hosted) { "Embedded attachments cannot be sent over network" }
    it.toFileVariant()
}.ifEmpty { null }

internal fun ChatMessageStatementContent.toChatMessageContent(): ChatMessage.Content {
    return when (this) {
        ChatMessageStatementContent.ContactAdded -> ChatMessage.Content.ContactAdded
        is ChatMessageStatementContent.CoinagePayment -> ChatMessage.Content.CoinagePayment(
            totalValue = totalValue,
            coinKeys = coinKeys,
            status = ChatMessage.Content.CoinagePayment.Status.Detecting
        )

        is ChatMessageStatementContent.Text -> ChatMessage.Content.Text(text)
        is ChatMessageStatementContent.Reply -> ownContent.toChatMessageContent()
        is ChatMessageStatementContent.Token -> ChatMessage.Content.Token(
            content.token.toDataByteArray(),
            content.platform.toOperatingSystem(),
            isVoIP = content.platform == TokenPlatform.IOS_VOIP
        )

        is ChatMessageStatementContent.Reacted -> ChatMessage.Content.Reacted(messageId, ChatMessageReactionContent(content.emoji))
        is ChatMessageStatementContent.ReactionRemoved -> ChatMessage.Content.ReactionRemoved(messageId, ChatMessageReactionContent(content.emoji))
        is ChatMessageStatementContent.Edited -> ChatMessage.Content.Edited(messageId, newContent.toChatMessageContent())
        is ChatMessageStatementContent.LeftChat -> ChatMessage.Content.LeftChat
        is ChatMessageStatementContent.ChatAccepted -> ChatMessage.Content.ChatAccepted(requestId)
        is ChatMessageStatementContent.DeviceChatAccepted -> ChatMessage.Content.DeviceChatAccepted(
            requestId = requestId,
            device = device.toDomain(),
        )
        is ChatMessageStatementContent.DataChannelAnswer -> ChatMessage.Content.DataChannelAnswer(offerMessageId, sdp)
        is ChatMessageStatementContent.DataChannelIceCandidate -> ChatMessage.Content.DataChannelIceCandidate(offerMessageId, sdp)
        is ChatMessageStatementContent.DataChannelOffer -> ChatMessage.Content.DataChannelOffer(sdp, purpose.toDomain())
        is ChatMessageStatementContent.DataChannelClosed -> ChatMessage.Content.DataChannelClosed(offerMessageId)
        is ChatMessageStatementContent.RichText -> content.toChatMessageContent()
        is ChatMessageStatementContent.DeviceAdded -> ChatMessage.Content.DeviceAdded(statementAccountId, encryptionPublicKey.requireX25519PublicKey())
        is ChatMessageStatementContent.DeviceRemoved -> ChatMessage.Content.DeviceRemoved(statementAccountId)
        is ChatMessageStatementContent.CompactionCommit -> ChatMessage.Content.CompactionCommit(
            claimIdentifier = claimIdentifier.toDataByteArray(),
            claimTicket = HopTicket.fromRaw(claimTicket),
            nodeUrl = node.toNodeUrl()
        )
    }
}

internal fun ChatMessageStatement.toChatMessage(
    authorAccountId: AccountId,
    contactAccountId: AccountId,
    messageStatus: ChatMessage.Status,
): ChatMessage {
    return when (val versioned = versioned) {
        is VersionedChatMessage.V1 -> versioned.message.toChatMessage(
            id,
            timestamp,
            authorAccountId,
            contactAccountId,
            messageStatus
        )
    }
}

private fun ChatMessageV1.toChatMessage(
    messageId: String,
    timestamp: ULong,
    authorAccountId: AccountId,
    contactAccountId: AccountId,
    messageStatus: ChatMessage.Status,
): ChatMessage {
    val extractedReplyId = (content as? ChatMessageStatementContent.Reply)?.messageId
    return ChatMessage(
        id = messageId,
        timestamp = timestamp.toLong(),
        origin = ChatMessageOrigin.fromContactChat(authorAccountId, contactAccountId),
        content = content.toChatMessageContent(),
        status = messageStatus,
        chatId = ChatId.fromContact(contactAccountId),
        replyToMessageId = extractedReplyId
    )
}

private fun OperatingSystem.toTokenPlatform(): TokenPlatform {
    return when (this) {
        OperatingSystem.IOS -> TokenPlatform.IOS
        OperatingSystem.ANDROID -> TokenPlatform.ANDROID
        OperatingSystem.UNKNOWN -> error("Should be one of supported OS for push tokens")
    }
}

private fun TokenPlatform.toOperatingSystem(): OperatingSystem {
    return when (this) {
        TokenPlatform.IOS -> OperatingSystem.IOS
        TokenPlatform.IOS_VOIP -> OperatingSystem.IOS
        TokenPlatform.ANDROID -> OperatingSystem.ANDROID
    }
}

private fun ChatMessageStatementContent.wrapInReplyOrSelf(replyTo: String?): ChatMessageStatementContent {
    if (replyTo == null) return this

    return when (this) {
        is ChatMessageStatementContent.Text -> ChatMessageStatementContent.Reply(
            messageId = replyTo,
            ownContent = toRichTextContent()
        )

        else -> this
    }
}

private fun ChatMessageStatementContent.Text.toRichTextContent(): RichTextContent {
    return RichTextContent(
        text = text,
        attachments = null
    )
}

private fun RichTextContent.toChatMessageContent(): ChatMessage.Content.RichText {
    return ChatMessage.Content.RichText(
        text = text,
        attachments = attachments?.map { it.toDomain() }.orEmpty()
    )
}

private fun Attachment.Hosted.toFileVariant(): FileVariant {
    return FileVariant.P2PMixnet(
        P2PMixnetFile(
            identifier = identifier.value,
            claimTicket = ticket.bytes,
            nodeEndpoint = NodeEndpoint.WssUrl(nodeUrl),
            meta = meta.toFileMeta()
        )
    )
}

private fun Attachment.Meta.toFileMeta(): FileMeta {
    val general = GeneralFileMeta(mimeType = mimeType, fileSize = size.inWholeBytes.toUInt())

    return when (this) {
        is Attachment.Meta.Image -> FileMeta.Image(
            ImageFileMeta(general, width.toUInt(), height.toUInt(), thumbnail = blurHash?.toThumbnail())
        )

        is Attachment.Meta.Video -> FileMeta.Video(
            VideoFileMeta(general, duration.toUInt(), thumbnail = blurHash?.toThumbnail())
        )

        is Attachment.Meta.General -> FileMeta.General(general)
    }
}

private fun FileVariant.toDomain(): Attachment.Hosted {
    return when (this) {
        is FileVariant.P2PMixnet -> Attachment.Hosted(
            uri = null,
            identifier = file.identifier.toDataByteArray(),
            ticket = HopTicket.fromRaw(file.claimTicket),
            nodeUrl = file.nodeEndpoint.toNodeUrl(),
            meta = file.meta.toDomain()
        )
    }
}

private fun NodeEndpoint.toNodeUrl(): String = when (this) {
    is NodeEndpoint.WssUrl -> url
}

private fun String.toThumbnail(): MediaThumbnail = toByteArray(Charsets.UTF_8)

private fun MediaThumbnail.toBlurHashString(): String = String(this, Charsets.UTF_8)

private fun FileMeta.toDomain(): Attachment.Meta {
    return when (this) {
        is FileMeta.Image -> Attachment.Meta.Image(
            width = image.width.toInt(),
            height = image.height.toInt(),
            blurHash = image.thumbnail?.toBlurHashString(),
            mimeType = image.general.mimeType,
            size = image.general.fileSize.toLong().bytes
        )

        is FileMeta.Video -> Attachment.Meta.Video(
            duration = video.duration.toLong(),
            blurHash = video.thumbnail?.toBlurHashString(),
            mimeType = video.general.mimeType,
            size = video.general.fileSize.toLong().bytes
        )

        is FileMeta.General -> Attachment.Meta.General(
            fileName = "", // according to spec, there's no file name in the general meta when sent over network
            mimeType = general.mimeType,
            size = general.fileSize.toLong().bytes
        )
    }
}

private fun DataChannelPurpose.toDomain() = when (this) {
    DataChannelPurpose.AUDIO_CALL -> ChatMessage.Content.DataChannelOffer.Purpose.AUDIO_CALL
    DataChannelPurpose.VIDEO_CALL -> ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL
}

private fun ChatMessage.Content.DataChannelOffer.Purpose.toDataChannelPurpose() = when (this) {
    ChatMessage.Content.DataChannelOffer.Purpose.AUDIO_CALL -> DataChannelPurpose.AUDIO_CALL
    ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL -> DataChannelPurpose.VIDEO_CALL
}

private fun DeviceInfo.toScale(): DeviceInfoScale {
    return DeviceInfoScale(
        statementAccountId = statementAccountId.value,
        encryptionPublicKey = encryptionPublicKey.toScale(),
    )
}

private fun DeviceInfoScale.toDomain(): DeviceInfo {
    return DeviceInfo(
        statementAccountId = statementAccountId.intoAccountId(),
        encryptionPublicKey = encryptionPublicKey.toDomain().getOrThrow(),
    )
}
