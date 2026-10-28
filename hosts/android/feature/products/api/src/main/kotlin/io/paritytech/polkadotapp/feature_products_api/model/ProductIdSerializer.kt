package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.KSerializer
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import uniffi.truapi.ProductId

object ProductIdSerializer : KSerializer<ProductId> {
    override val descriptor = PrimitiveSerialDescriptor("ProductId", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: ProductId) = encoder.encodeString(value.value)

    override fun deserialize(decoder: Decoder): ProductId = ProductId.parse(decoder.decodeString()).getOrThrow()
}
