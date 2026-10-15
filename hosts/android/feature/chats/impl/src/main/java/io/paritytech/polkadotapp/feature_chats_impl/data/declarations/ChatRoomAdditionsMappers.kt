package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBranchPositionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatReasoningModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadParentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadSummaryModel
import kotlinx.serialization.encodeToByteArray
import timber.log.Timber

/** The version byte of the first stored shape of the additions. */
private const val ADDITIONS_FORMAT_V1: Byte = 1

fun ChatRoomFeaturesModel.encode(): ByteArray = encodeV1(toLocal())

fun ChatMessageMetaModel.encode(): ByteArray = encodeV1(toLocal())

fun ByteArray.decodeFeatures(): ChatRoomFeaturesModel? = decodeV1<ChatRoomFeaturesLocal>("features")?.toDomain()

fun ByteArray.decodeMessageMeta(): ChatMessageMetaModel? = decodeV1<ChatMessageMetaValueLocal>("message meta")?.toDomain()

private inline fun <reified T> encodeV1(value: T): ByteArray = byteArrayOf(ADDITIONS_FORMAT_V1) + BinaryScale.encodeToByteArray(value)

// A value of another version or that does not decode reads as "not declared", so the host default applies.
private inline fun <reified T> ByteArray.decodeV1(part: String): T? {
    if (firstOrNull() != ADDITIONS_FORMAT_V1) {
        Timber.w("Cannot read the %s of a chat room: version %s", part, firstOrNull())
        return null
    }
    return runCatching { BinaryScale.decodeFromByteArray<T>(copyOfRange(1, size)) }
        .onFailure { Timber.w(it, "Cannot decode the %s of a chat room", part) }
        .getOrNull()
}

fun ChatRoomFeaturesModel.toLocal() = ChatRoomFeaturesLocal(
    preimageAttachments = preimageAttachments,
    search = search,
    calls = calls?.let { ChatRoomCallsLocal(it.audio, it.video) },
    threadOf = threadOf?.let { ChatThreadParentLocal(it.roomId, it.messageId) },
    readAloud = readAloud,
)

fun ChatRoomFeaturesLocal.toDomain() = ChatRoomFeaturesModel(
    preimageAttachments = preimageAttachments,
    search = search,
    calls = calls?.let { ChatRoomCallsModel(it.audio, it.video) },
    threadOf = threadOf?.let { ChatThreadParentModel(it.roomId, it.messageId) },
    readAloud = readAloud,
)

fun ChatMessageMetaModel.toLocal() = ChatMessageMetaValueLocal(
    thread = thread?.let {
        ChatThreadSummaryLocal(it.roomId, it.replyCount.toUInt().toInt(), it.lastReplyAt, it.participantIds)
    },
    branch = branch?.let { ChatBranchPositionLocal(it.index.toShort(), it.count.toShort()) },
    reasoning = reasoning?.let { ChatReasoningLocal(it.durationMs?.toUInt()?.toInt()) },
)

fun ChatMessageMetaValueLocal.toDomain() = ChatMessageMetaModel(
    thread = thread?.let {
        ChatThreadSummaryModel(it.roomId, it.replyCount.toUInt().toLong(), it.lastReplyAt, it.participantIds)
    },
    branch = branch?.let { ChatBranchPositionModel(it.index.toUShort().toInt(), it.count.toUShort().toInt()) },
    reasoning = reasoning?.let { ChatReasoningModel(it.durationMs?.toUInt()?.toLong()) },
)
