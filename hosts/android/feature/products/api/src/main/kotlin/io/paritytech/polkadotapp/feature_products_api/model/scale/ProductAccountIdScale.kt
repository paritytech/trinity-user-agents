@file:UseSerializers(ProductIdSerializer::class)

package io.paritytech.polkadotapp.feature_products_api.model.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductIdSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.UseSerializers
import uniffi.truapi.ProductId

@Serializable
@AsTuple
data class ProductAccountIdScale(
    val dotNsIdentifier: ProductId,
    val derivationIndex: ProductDerivationIndexScale,
)


fun ProductAccountId.toScale(): ProductAccountIdScale {
    return ProductAccountIdScale(productId, index.toScale())
}

fun ProductAccountIdScale.toDomain(): Result<ProductAccountId> {
    return derivationIndex.toDomain().map { ProductAccountId(dotNsIdentifier, it) }
}
