package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization

import com.google.gson.JsonArray
import com.google.gson.JsonDeserializationContext
import com.google.gson.JsonDeserializer
import com.google.gson.JsonElement
import com.google.gson.JsonPrimitive
import com.google.gson.JsonSerializationContext
import com.google.gson.JsonSerializer
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import java.lang.reflect.Type

class ProductAccountIdTupleAdapter : JsonSerializer<ProductAccountId>, JsonDeserializer<ProductAccountId> {
    private val indexAdapter = DerivationIndexWireAdapter()

    override fun serialize(src: ProductAccountId, typeOfSrc: Type, context: JsonSerializationContext): JsonElement {
        return JsonArray().apply {
            add(JsonPrimitive(src.productId))
            add(indexAdapter.serialize(src.index.toWire(), DerivationIndexWire::class.java, context))
        }
    }

    override fun deserialize(json: JsonElement, typeOfT: Type, context: JsonDeserializationContext): ProductAccountId {
        require(json is JsonArray && json.size() == 2) {
            "ProductAccountId wire form must be a 2-element JSON array [productId, derivationIndex]"
        }

        val index = indexAdapter.deserialize(json[1], DerivationIndexWire::class.java, context)

        return ProductAccountId(
            productId = json[0].asString,
            index = index.toDomain().getOrThrow(),
        )
    }
}
