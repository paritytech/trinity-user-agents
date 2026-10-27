package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization

import com.google.gson.TypeAdapter
import com.google.gson.stream.JsonReader
import com.google.gson.stream.JsonWriter
import uniffi.truapi.ProductId

class ProductIdJsonAdapter : TypeAdapter<ProductId>() {
    override fun write(writer: JsonWriter, value: ProductId) {
        writer.value(value.value)
    }

    override fun read(reader: JsonReader): ProductId = ProductId.parse(reader.nextString())
}
