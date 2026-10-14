package io.paritytech.polkadotapp.feature_products_api.model.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import kotlinx.serialization.Serializable

@Serializable
@AsTuple
data class ProductAccountIdScale(
    val dotNsIdentifier: ProductIdScale,
    val derivationIndex: ProductDerivationIndexScale,
)

typealias DotNsIdentifierScale = String
typealias ProductIdScale = DotNsIdentifierScale

fun ProductAccountId.toScale(): ProductAccountIdScale {
    return ProductAccountIdScale(productId, index.toScale())
}

fun ProductAccountIdScale.toDomain(): Result<ProductAccountId> {
    return derivationIndex.toDomain().map { ProductAccountId(dotNsIdentifier, it) }
}
