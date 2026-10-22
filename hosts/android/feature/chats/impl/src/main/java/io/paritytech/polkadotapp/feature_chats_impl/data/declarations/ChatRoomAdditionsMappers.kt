package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import kotlinx.serialization.encodeToByteArray
import timber.log.Timber

/** The version byte of the first stored shape of the additions. */
private const val ADDITIONS_FORMAT_V1: Byte = 1

/** The version byte of the shape without the features that Chat Extension v2 dropped. */
private const val ADDITIONS_FORMAT_V2: Byte = 2

fun ChatRoomFeaturesModel.encode(): ByteArray = byteArrayOf(ADDITIONS_FORMAT_V2) + BinaryScale.encodeToByteArray(toLocal())

// A value of another version or that does not decode reads as "not declared", so the host default applies.
fun ByteArray.decodeFeatures(): ChatRoomFeaturesModel? {
    val body = copyOfRange(1, size.coerceAtLeast(1))
    val decoded = when (firstOrNull()) {
        ADDITIONS_FORMAT_V1 -> runCatching { BinaryScale.decodeFromByteArray<ChatRoomFeaturesLocalV1>(body).toDomain() }
        ADDITIONS_FORMAT_V2 -> runCatching { BinaryScale.decodeFromByteArray<ChatRoomFeaturesLocal>(body).toDomain() }
        else -> Result.failure(IllegalArgumentException("version ${firstOrNull()}"))
    }
    return decoded.onFailure { Timber.w(it, "Cannot decode the features of a chat room") }.getOrNull()
}

fun ChatRoomFeaturesModel.toLocal() = ChatRoomFeaturesLocal(
    calls = calls?.let { ChatRoomCallsLocal(it.audio, it.video) },
    hopAttachments = hopAttachments,
)

fun ChatRoomFeaturesLocal.toDomain() = ChatRoomFeaturesModel(
    calls = calls?.let { ChatRoomCallsModel(it.audio, it.video) },
    hopAttachments = hopAttachments,
)

private fun ChatRoomFeaturesLocalV1.toDomain() = ChatRoomFeaturesModel(
    calls = calls?.let { ChatRoomCallsModel(it.audio, it.video) },
    hopAttachments = hopAttachments,
)
