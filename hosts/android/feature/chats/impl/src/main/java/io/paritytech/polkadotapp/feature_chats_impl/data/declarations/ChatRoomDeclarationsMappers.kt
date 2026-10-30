package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.database.model.ChatRoomDeclarationsLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
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

// A preview message keeps the stored content of a message. The renderer of the product that sent a custom message
// encodes and decodes its content, as for a message in the room.
fun ChatRoomDeclarationsLocal.toDomain(decoder: CustomContentDecoder): ChatRoomDeclarations = ChatRoomDeclarations(
    config = config?.decodeOrNull<ChatRoomConfigLocal>("config")?.toDomain(),
    configGeneration = configGeneration,
    metadata = ChatRoomMetadataModel(description = description, placeholder = placeholder),
    preview = preview?.decodePreviewOrNull()?.toDomain(decoder),
)

fun ChatRoomConfigModel.encode(): ByteArray = BinaryScale.encodeToByteArray(toLocal())

fun ChatRoomPreviewModel.encode(decoder: CustomContentDecoder): ByteArray = BinaryScale.encodeToByteArray(toLocal(decoder))

// A value that does not decode reads as "not declared", so the host default applies until the product declares again.
private inline fun <reified T> ByteArray.decodeOrNull(part: String): T? =
    runCatching { BinaryScale.decodeFromByteArray<T>(this) }.orNotDeclared(part)

// A preview in its first layout reads as not pinned and without an activity.
private fun ByteArray.decodePreviewOrNull(): ChatRoomPreviewLocal? =
    runCatching { BinaryScale.decodeFromByteArray<ChatRoomPreviewLocal>(this) }
        .recoverCatching { BinaryScale.decodeFromByteArray<ChatRoomPreviewFirstLayoutLocal>(this).toCurrentLayout() }
        .orNotDeclared("preview")

private fun <T> Result<T>.orNotDeclared(part: String): T? =
    onFailure { Timber.w(it, "Cannot decode the %s of a chat room", part) }.getOrNull()

private fun ChatRoomPreviewFirstLayoutLocal.toCurrentLayout() = ChatRoomPreviewLocal(
    timestamp = timestamp,
    badge = badge,
    lastMessage = lastMessage,
    lastReaction = lastReaction,
    unseenReactions = unseenReactions,
    pinned = false,
    activity = null,
)

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

fun ChatRoomPreviewModel.toLocal(decoder: CustomContentDecoder) = ChatRoomPreviewLocal(
    timestamp = timestamp,
    badge = badge?.let {
        when (it) {
            is ChatRoomBadgeModel.Count -> ChatRoomBadgeLocal.UnreadCount(it.count)
            is ChatRoomBadgeModel.Image -> ChatRoomBadgeLocal.Image(it.icon)
        }
    },
    lastMessage = lastMessage?.toPreviewLocal(decoder),
    lastReaction = lastReaction?.let { PreviewReactionLocal(it.emoji, it.byUser, it.message?.toPreviewLocal(decoder), it.removed) },
    unseenReactions = unseenReactions,
    pinned = pinned,
    activity = activity?.let { ChatRoomActivityLocal(it.text, it.ongoing) },
)

fun ChatRoomPreviewLocal.toDomain(decoder: CustomContentDecoder) = ChatRoomPreviewModel(
    timestamp = timestamp,
    badge = badge?.let {
        when (it) {
            ChatRoomBadgeLocal.Hidden -> ChatRoomBadgeModel.Count(0)
            is ChatRoomBadgeLocal.UnreadCount -> ChatRoomBadgeModel.Count(it.count)
            is ChatRoomBadgeLocal.Image -> ChatRoomBadgeModel.Image(it.icon)
        }
    },
    lastMessage = lastMessage?.toDomain(decoder),
    lastReaction = lastReaction?.let { ChatPreviewReactionModel(it.emoji, it.byUser, it.message?.toDomain(decoder), it.removed) },
    unseenReactions = unseenReactions,
    pinned = pinned,
    activity = activity?.let { ChatRoomActivityModel(it.text, it.ongoing) },
)

private fun ProductRoomMessage.toPreviewLocal(decoder: CustomContentDecoder) = PreviewMessageLocal(
    messageId = messageId,
    participantId = participantId,
    timestamp = timestamp,
    content = content.toData(decoder),
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

private fun PreviewMessageLocal.toDomain(decoder: CustomContentDecoder) = ProductRoomMessage(
    messageId = messageId,
    participantId = participantId,
    timestamp = timestamp,
    content = content.toDomain(decoder),
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
