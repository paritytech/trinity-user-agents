package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isInternal
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toData
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toDomain
import kotlinx.serialization.encodeToByteArray
import timber.log.Timber
import kotlin.reflect.KClass

fun ChatMessage.Content.toEncodedByteArray(
    customContentDecoder: CustomContentDecoder,
): ByteArray {
    return BinaryScale.encodeToByteArray(toData(customContentDecoder))
}

fun ChatMessage.toLocal(
    customContentDecoder: CustomContentDecoder,
): ChatMessageLocal {
    val (contentData, type) = content.toLocal(customContentDecoder)

    return ChatMessageLocal(
        id = id,
        timestamp = timestamp,
        updatedAt = System.currentTimeMillis(),
        sortOrder = ChatMessageLocal.UNORDERED,
        origin = origin.toLocal(),
        status = status.toLocal(),
        type = type,
        searchableContent = content.toSearchableContent(),
        content = contentData,
        chatId = chatId.toLocal(),
        replyToMessageId = replyToMessageId,
        isInternal = content.isInternal()
    )
}

fun ChatMessage.Content.toLocal(
    customContentDecoder: CustomContentDecoder,
): Pair<ByteArray, ChatMessageLocal.Type> {
    val type = toLocalType()
    val local = toData(customContentDecoder)
    val contentData = BinaryScale.encodeToByteArray<ChatMessageContentLocal>(local)

    return contentData to type
}

fun ChatMessageLocal.toDomain(
    customContentDecoder: CustomContentDecoder,
): ChatMessage {
    val content = runCatching {
        BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(content)
    }.getOrElse {
        Timber.w(it, "Failed to decode content for ChatMessageContentLocal: ${content.toHexString(withPrefix = true)}")
        ChatMessageContentLocal.Unsupported(content)
    }

    return ChatMessage(
        id = id,
        timestamp = timestamp,
        chatId = ChatId.fromRawValue(chatId),
        origin = origin.toDomain(),
        content = content.toDomain(customContentDecoder),
        status = status.toDomain(),
        replyToMessageId = replyToMessageId
    )
}

fun ChatId.toLocal(): ByteArray {
    return value.value
}

fun ChatMessageOrigin.toLocal(): ChatMessageLocal.Origin {
    return when (this) {
        is ChatMessageOrigin.Contact -> ChatMessageLocal.Origin(
            type = ChatMessageLocal.OriginType.CONTACT,
            key = contactAccountId.value
        )

        is ChatMessageOrigin.Extension -> ChatMessageLocal.Origin(
            type = ChatMessageLocal.OriginType.MIDDLEWARE,
            key = extensionId.encodeToByteArray()
        )

        is ChatMessageOrigin.User -> ChatMessageLocal.Origin(
            type = ChatMessageLocal.OriginType.USER,
            key = null
        )
    }
}

fun ChatMessageLocal.Origin.toDomain(): ChatMessageOrigin {
    return when (type) {
        ChatMessageLocal.OriginType.USER -> ChatMessageOrigin.User
        ChatMessageLocal.OriginType.CONTACT -> ChatMessageOrigin.Contact(key!!.intoAccountId())
        ChatMessageLocal.OriginType.MIDDLEWARE -> ChatMessageOrigin.Extension(key!!.decodeToString())
    }
}

internal fun ChatMessage.Content.toLocalType(): ChatMessageLocal.Type {
    return when (this) {
        is ChatMessage.Content.Text -> ChatMessageLocal.Type.TEXT
        is ChatMessage.Content.Token -> ChatMessageLocal.Type.TOKEN
        is ChatMessage.Content.CoinagePayment -> ChatMessageLocal.Type.PAYMENT
        is ChatMessage.Content.ContactAdded -> ChatMessageLocal.Type.CONTACT_ADDED
        is ChatMessage.Content.RichText -> ChatMessageLocal.Type.RICH_TEXT
        is ChatMessage.Content.Reacted -> ChatMessageLocal.Type.REACTED
        is ChatMessage.Content.ReactionRemoved -> ChatMessageLocal.Type.REACTION_REMOVED
        is ChatMessage.Content.Unsupported -> ChatMessageLocal.Type.UNSUPPORTED
        is ChatMessage.Content.LeftChat -> ChatMessageLocal.Type.LEFT_CHAT
        is ChatMessage.Content.Edited -> ChatMessageLocal.Type.EDITED
        is ChatMessage.Content.Custom<*> -> ChatMessageLocal.Type.CUSTOM
        is ChatMessage.Content.ChatAccepted -> ChatMessageLocal.Type.CHAT_ACCEPTED
        is ChatMessage.Content.DeviceChatAccepted -> ChatMessageLocal.Type.CHAT_ACCEPTED
        is ChatMessage.Content.ChatRequest -> ChatMessageLocal.Type.CHAT_REQUEST
        is ChatMessage.Content.DataChannelAnswer -> ChatMessageLocal.Type.DATA_CHANNEL_ANSWER
        is ChatMessage.Content.DataChannelIceCandidate -> ChatMessageLocal.Type.DATA_CHANNEL_CANDIDATE
        is ChatMessage.Content.DataChannelOffer -> ChatMessageLocal.Type.DATA_CHANNEL_OFFER
        is ChatMessage.Content.DataChannelClosed -> ChatMessageLocal.Type.DATA_CHANNEL_CLOSED
        is ChatMessage.Content.DeviceAdded -> ChatMessageLocal.Type.DEVICE_ADDED
        is ChatMessage.Content.DeviceRemoved -> ChatMessageLocal.Type.DEVICE_REMOVED
        is ChatMessage.Content.CompactionCommit -> ChatMessageLocal.Type.COMPACTION_COMMIT
        is ChatMessage.Content.CompactionUnavailable -> ChatMessageLocal.Type.COMPACTION_UNAVAILABLE
    }
}

internal fun KClass<out ChatMessage.Content>.toLocalType(): ChatMessageLocal.Type {
    return when (this) {
        ChatMessage.Content.Text::class -> ChatMessageLocal.Type.TEXT
        ChatMessage.Content.Token::class -> ChatMessageLocal.Type.TOKEN
        ChatMessage.Content.CoinagePayment::class -> ChatMessageLocal.Type.PAYMENT
        ChatMessage.Content.ContactAdded::class -> ChatMessageLocal.Type.CONTACT_ADDED
        ChatMessage.Content.RichText::class -> ChatMessageLocal.Type.RICH_TEXT
        ChatMessage.Content.Reacted::class -> ChatMessageLocal.Type.REACTED
        ChatMessage.Content.ReactionRemoved::class -> ChatMessageLocal.Type.REACTION_REMOVED
        ChatMessage.Content.Unsupported::class -> ChatMessageLocal.Type.UNSUPPORTED
        ChatMessage.Content.LeftChat::class -> ChatMessageLocal.Type.LEFT_CHAT
        ChatMessage.Content.Edited::class -> ChatMessageLocal.Type.EDITED
        ChatMessage.Content.Custom::class -> ChatMessageLocal.Type.CUSTOM
        ChatMessage.Content.ChatAccepted::class -> ChatMessageLocal.Type.CHAT_ACCEPTED
        ChatMessage.Content.DeviceChatAccepted::class -> ChatMessageLocal.Type.CHAT_ACCEPTED
        ChatMessage.Content.ChatRequest::class -> ChatMessageLocal.Type.CHAT_REQUEST
        ChatMessage.Content.DataChannelAnswer::class -> ChatMessageLocal.Type.DATA_CHANNEL_ANSWER
        ChatMessage.Content.DataChannelIceCandidate::class -> ChatMessageLocal.Type.DATA_CHANNEL_CANDIDATE
        ChatMessage.Content.DataChannelOffer::class -> ChatMessageLocal.Type.DATA_CHANNEL_OFFER
        ChatMessage.Content.DataChannelClosed::class -> ChatMessageLocal.Type.DATA_CHANNEL_CLOSED
        ChatMessage.Content.DeviceAdded::class -> ChatMessageLocal.Type.DEVICE_ADDED
        ChatMessage.Content.DeviceRemoved::class -> ChatMessageLocal.Type.DEVICE_REMOVED
        ChatMessage.Content.CompactionCommit::class -> ChatMessageLocal.Type.COMPACTION_COMMIT
        ChatMessage.Content.CompactionUnavailable::class -> ChatMessageLocal.Type.COMPACTION_UNAVAILABLE
        else -> error("Unknown content type: $this")
    }
}

private fun ChatMessage.Content.toSearchableContent(): String {
    return when (this) {
        is ChatMessage.Content.Text -> text
        is ChatMessage.Content.RichText -> text.orEmpty()
        is ChatMessage.Content.Edited -> content.text.orEmpty()
        is ChatMessage.Content.ChatRequest -> welcome?.text.orEmpty()

        ChatMessage.Content.LeftChat,
        ChatMessage.Content.ContactAdded,
        is ChatMessage.Content.CoinagePayment,
        is ChatMessage.Content.Reacted,
        is ChatMessage.Content.ReactionRemoved,
        is ChatMessage.Content.Token,
        is ChatMessage.Content.Unsupported,
        is ChatMessage.Content.DataChannelAnswer,
        is ChatMessage.Content.DataChannelIceCandidate,
        is ChatMessage.Content.DataChannelOffer,
        is ChatMessage.Content.DataChannelClosed,
        is ChatMessage.Content.DeviceAdded,
        is ChatMessage.Content.DeviceRemoved,
        is ChatMessage.Content.Custom<*>,
        is ChatMessage.Content.ChatAccepted,
        is ChatMessage.Content.DeviceChatAccepted,
        is ChatMessage.Content.CompactionCommit,
        is ChatMessage.Content.CompactionUnavailable -> ""
    }
}

fun ChatMessage.Status.toLocal(): ChatMessageLocal.Status {
    return when (this) {
        ChatMessage.Status.PROCESSING -> ChatMessageLocal.Status.PROCESSING
        ChatMessage.Status.NEW -> ChatMessageLocal.Status.NEW
        ChatMessage.Status.IS_SENT -> ChatMessageLocal.Status.IS_SENT
        ChatMessage.Status.IS_READ -> ChatMessageLocal.Status.IS_READ
        ChatMessage.Status.DELIVERY_FAILED -> ChatMessageLocal.Status.DELIVERY_FAILED
    }
}

internal fun ChatMessageLocal.Status.toDomain(): ChatMessage.Status {
    return when (this) {
        ChatMessageLocal.Status.PROCESSING -> ChatMessage.Status.PROCESSING
        ChatMessageLocal.Status.NEW -> ChatMessage.Status.NEW
        ChatMessageLocal.Status.IS_SENT -> ChatMessage.Status.IS_SENT
        ChatMessageLocal.Status.IS_READ -> ChatMessage.Status.IS_READ
        ChatMessageLocal.Status.DELIVERY_FAILED -> ChatMessage.Status.DELIVERY_FAILED
    }
}
