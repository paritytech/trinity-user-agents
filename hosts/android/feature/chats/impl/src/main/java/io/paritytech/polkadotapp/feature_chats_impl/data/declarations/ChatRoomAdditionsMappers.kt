package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import kotlinx.serialization.encodeToByteArray
import timber.log.Timber

/** The version byte of the first stored shape of the additions. */
private const val ADDITIONS_FORMAT_V1: Byte = 1

fun ChatRoomFeaturesModel.encode(): ByteArray = byteArrayOf(ADDITIONS_FORMAT_V1) + BinaryScale.encodeToByteArray(toLocal())

// A value of another version or that does not decode reads as "not declared", so the host default applies.
fun ByteArray.decodeFeatures(): ChatRoomFeaturesModel? {
    if (firstOrNull() != ADDITIONS_FORMAT_V1) {
        Timber.w("Cannot read the features of a chat room: version %s", firstOrNull())
        return null
    }
    return runCatching { BinaryScale.decodeFromByteArray<ChatRoomFeaturesLocal>(copyOfRange(1, size)) }
        .onFailure { Timber.w(it, "Cannot decode the features of a chat room") }
        .getOrNull()
        ?.toDomain()
}

fun ChatRoomFeaturesModel.toLocal() = ChatRoomFeaturesLocal(
    preimageAttachments = preimageAttachments,
    search = search,
    calls = calls?.let { ChatRoomCallsLocal(it.audio, it.video) },
    readAloud = readAloud,
    sealedAttachments = sealedAttachments,
    hopAttachments = hopAttachments,
)

fun ChatRoomFeaturesLocal.toDomain() = ChatRoomFeaturesModel(
    preimageAttachments = preimageAttachments,
    search = search,
    calls = calls?.let { ChatRoomCallsModel(it.audio, it.video) },
    readAloud = readAloud,
    sealedAttachments = sealedAttachments,
    hopAttachments = hopAttachments,
)
