package io.paritytech.polkadotapp.common.utils

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

inline fun <reified T> BinaryScale.encodeToByteArrayCatching(value: T) = runCatching {
    encodeToByteArray(value)
}

inline fun <reified T> BinaryScale.decodeFromByteArrayCatching(bytes: ByteArray) = runCatching {
    decodeFromByteArray<T>(bytes)
}
