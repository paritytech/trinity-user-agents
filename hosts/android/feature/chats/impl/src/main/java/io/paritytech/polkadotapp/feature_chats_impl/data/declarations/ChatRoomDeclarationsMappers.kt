package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.model.ChatRoomDeclarationsLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRendererId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toData
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toDomain
import kotlinx.serialization.encodeToByteArray
import timber.log.Timber

fun ChatRoomDeclarationsLocal.toDomain(): ChatRoomDeclarations = ChatRoomDeclarations(
    config = config?.decodeOrNull<ChatRoomConfigLocal>("config")?.toDomain(),
    configGeneration = configGeneration,
    metadata = ChatRoomMetadataModel(description = description),
    preview = preview?.decodeOrNull<ChatRoomPreviewLocal>("preview")?.toDomain(),
    features = features?.decodeFeatures(),
    contact = contact?.decodeOrNull<ChatRoomContactLocal>("contact")?.toDomain(),
)

fun ChatRoomConfigModel.encode(): ByteArray = BinaryScale.encodeToByteArray(toLocal())

fun ChatRoomPreviewModel.encode(): ByteArray = BinaryScale.encodeToByteArray(toLocal())

fun ChatRoomContactModel.encode(): ByteArray = BinaryScale.encodeToByteArray(toLocal())

// A value that does not decode reads as "not declared", so the host default applies until the product declares again.
private inline fun <reified T> ByteArray.decodeOrNull(part: String): T? =
    runCatching { BinaryScale.decodeFromByteArray<T>(this) }
        .onFailure { Timber.w(it, "Cannot decode the %s of a chat room", part) }
        .getOrNull()

fun ChatRoomConfigModel.toLocal() = ChatRoomConfigLocal(
    productSource = productSource,
    reactionDetails = reactionDetails,
    participants = false,
    messageActions = false,
    allowedReactions = null,
    placeholder = null,
    reportSeen = reportSeen,
)

fun ChatRoomConfigLocal.toDomain() = ChatRoomConfigModel(
    productSource = productSource,
    reactionDetails = reactionDetails,
    reportSeen = reportSeen,
)

fun ChatRoomPreviewModel.toLocal() = ChatRoomPreviewLocal(
    timestamp = timestamp,
    badge = badge?.let {
        when (it) {
            ChatRoomBadgeModel.Hidden -> ChatRoomBadgeLocal.Hidden
            is ChatRoomBadgeModel.Count -> ChatRoomBadgeLocal.UnreadCount(it.count)
        }
    },
    lastMessage = lastMessage?.toPreviewLocal(),
    lastReaction = lastReaction?.let { PreviewReactionLocal(it.emoji, it.byUser, it.message?.toPreviewLocal(), it.removed) },
    unseenReactions = unseenReactions,
)

fun ChatRoomPreviewLocal.toDomain() = ChatRoomPreviewModel(
    timestamp = timestamp,
    badge = badge?.let {
        when (it) {
            ChatRoomBadgeLocal.Hidden -> ChatRoomBadgeModel.Hidden
            is ChatRoomBadgeLocal.UnreadCount -> ChatRoomBadgeModel.Count(it.count)
        }
    },
    lastMessage = lastMessage?.toDomain(),
    lastReaction = lastReaction?.let { ChatPreviewReactionModel(it.emoji, it.byUser, it.message?.toDomain(), it.removed) },
    unseenReactions = unseenReactions,
)

// A preview message never has a body that a product draws itself.
private object NoCustomContent : CustomContentDecoder {
    override fun decode(rendererId: CustomChatMessageRendererId, content: ByteArray): Result<Any?> =
        Result.failure(IllegalStateException("A preview message has no custom content"))

    override fun encode(rendererId: CustomChatMessageRendererId, value: Any?): Result<ByteArray> =
        Result.failure(IllegalStateException("A preview message has no custom content"))
}

private fun ProductRoomMessage.toPreviewLocal() = PreviewMessageLocal(
    messageId = messageId,
    participantId = participantId,
    timestamp = timestamp,
    content = content.toData(NoCustomContent),
    status = when (status) {
        ProductMessageStatusModel.PREPARING -> PreviewMessageStatusLocal.PREPARING
        ProductMessageStatusModel.SENT -> PreviewMessageStatusLocal.SENT
        ProductMessageStatusModel.DELIVERED -> PreviewMessageStatusLocal.DELIVERED
        ProductMessageStatusModel.READ -> PreviewMessageStatusLocal.READ
        ProductMessageStatusModel.FAILED -> PreviewMessageStatusLocal.FAILED
    },
    replyTo = replyTo,
    editedAt = editedAt,
    request = request,
)

private fun PreviewMessageLocal.toDomain() = ProductRoomMessage(
    messageId = messageId,
    participantId = participantId,
    timestamp = timestamp,
    content = content.toDomain(NoCustomContent),
    status = when (status) {
        PreviewMessageStatusLocal.PREPARING -> ProductMessageStatusModel.PREPARING
        PreviewMessageStatusLocal.SENT -> ProductMessageStatusModel.SENT
        PreviewMessageStatusLocal.DELIVERED -> ProductMessageStatusModel.DELIVERED
        PreviewMessageStatusLocal.READ -> ProductMessageStatusModel.READ
        PreviewMessageStatusLocal.FAILED -> ProductMessageStatusModel.FAILED
    },
    replyTo = replyTo,
    editedAt = editedAt,
    reactions = emptyList(),
    request = request,
)

fun ChatRoomContactModel.toLocal() = ChatRoomContactLocal(
    account = account.value,
    username = username,
    state = when (state) {
        ChatContactStateModel.INVITE -> ChatContactStateLocal.INVITE
        ChatContactStateModel.OUTGOING -> ChatContactStateLocal.OUTGOING
        ChatContactStateModel.INCOMING -> ChatContactStateLocal.INCOMING
        ChatContactStateModel.ESTABLISHED -> ChatContactStateLocal.ESTABLISHED
        ChatContactStateModel.BLOCKED -> ChatContactStateLocal.BLOCKED
        ChatContactStateModel.PEER_LEFT -> ChatContactStateLocal.PEER_LEFT
    },
)

fun ChatRoomContactLocal.toDomain() = ChatRoomContactModel(
    account = account.toDataByteArray(),
    username = username,
    state = when (state) {
        ChatContactStateLocal.INVITE -> ChatContactStateModel.INVITE
        ChatContactStateLocal.OUTGOING -> ChatContactStateModel.OUTGOING
        ChatContactStateLocal.INCOMING -> ChatContactStateModel.INCOMING
        ChatContactStateLocal.ESTABLISHED -> ChatContactStateModel.ESTABLISHED
        ChatContactStateLocal.BLOCKED -> ChatContactStateModel.BLOCKED
        ChatContactStateLocal.PEER_LEFT -> ChatContactStateModel.PEER_LEFT
    },
)
