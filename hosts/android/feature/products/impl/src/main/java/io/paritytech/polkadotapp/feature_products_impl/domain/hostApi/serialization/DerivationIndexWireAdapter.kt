package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization

import com.google.gson.JsonDeserializationContext
import com.google.gson.JsonDeserializer
import com.google.gson.JsonElement
import com.google.gson.JsonPrimitive
import com.google.gson.JsonSerializationContext
import com.google.gson.JsonSerializer
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import java.lang.reflect.Type

class DerivationIndexWireAdapter : JsonSerializer<DerivationIndexWire>, JsonDeserializer<DerivationIndexWire> {
    override fun serialize(src: DerivationIndexWire, typeOfSrc: Type, context: JsonSerializationContext): JsonElement {
        return when (src) {
            is DerivationIndexWire.Plain -> JsonPrimitive(src.index.toLong())
            is DerivationIndexWire.Raw -> JsonPrimitive(src.bytes.value.toHexString(withPrefix = true))
        }
    }

    override fun deserialize(json: JsonElement, typeOfT: Type, context: JsonDeserializationContext): DerivationIndexWire {
        require(json is JsonPrimitive) { "Derivation index must be a number or a hex string, got: $json" }

        return if (json.isNumber) {
            DerivationIndexWire.Plain(json.asLong.toUInt())
        } else {
            DerivationIndexWire.Raw(DataByteArray.fromHex(json.asString))
        }
    }
}
