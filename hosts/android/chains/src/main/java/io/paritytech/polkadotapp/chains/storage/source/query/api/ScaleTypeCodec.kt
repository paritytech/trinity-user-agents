package io.paritytech.polkadotapp.chains.storage.source.query.api

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArrayDynamicStructSerializer
import kotlinx.serialization.KSerializer
import kotlinx.serialization.serializer
import kotlin.reflect.KType
import kotlin.reflect.typeOf

class ScaleTypeCodec<T>(type: KType) {
    private val serializer = resolveSerializer<T>(type)

    fun decode(instance: Any?): T {
        return Scale.decode(serializer, instance)
    }

    fun encode(value: T): Any? {
        return Scale.encode(serializer, value)
    }
}

// Mirrors the SDK's own lookup, which swaps the built-in ByteArray serializer for the dynamic-struct one.
@Suppress("UNCHECKED_CAST")
private fun <T> resolveSerializer(type: KType): KSerializer<T> {
    val serializer = if (type == typeOf<ByteArray>()) ByteArrayDynamicStructSerializer else Scale.serializersModule.serializer(type)

    return serializer as KSerializer<T>
}
