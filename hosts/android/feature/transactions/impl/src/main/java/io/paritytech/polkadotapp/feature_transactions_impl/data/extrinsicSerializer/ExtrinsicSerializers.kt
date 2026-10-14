package io.paritytech.polkadotapp.feature_transactions_impl.data.extrinsicSerializer

import com.google.gson.Gson
import com.google.gson.GsonBuilder
import com.google.gson.JsonDeserializationContext
import com.google.gson.JsonDeserializer
import com.google.gson.JsonElement
import com.google.gson.JsonObject
import com.google.gson.JsonPrimitive
import com.google.gson.JsonSerializationContext
import com.google.gson.JsonSerializer
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import java.lang.reflect.Type

class ByteArrayHexAdapter : JsonSerializer<ByteArray>, JsonDeserializer<ByteArray> {
    override fun serialize(src: ByteArray, typeOfSrc: Type, context: JsonSerializationContext): JsonElement {
        return JsonPrimitive(src.toHexString(withPrefix = true))
    }

    override fun deserialize(json: JsonElement, typeOfT: Type, context: JsonDeserializationContext): ByteArray {
        return json.asString.fromHex()
    }
}

private class GenericCallAdapter : JsonSerializer<GenericCall.Instance> {
    override fun serialize(src: GenericCall.Instance, typeOfSrc: Type, context: JsonSerializationContext): JsonElement {
        return JsonObject().apply {
            add("module", JsonPrimitive(src.module.name))
            add("function", JsonPrimitive(src.function.name))
            add("args", context.serialize(src.arguments))
        }
    }
}

object ExtrinsicSerializers {
    fun gson(): Gson = GsonBuilder()
        .registerTypeHierarchyAdapter(ByteArray::class.java, ByteArrayHexAdapter())
        .registerTypeHierarchyAdapter(GenericCall.Instance::class.java, GenericCallAdapter())
        .setPrettyPrinting()
        .create()
}
