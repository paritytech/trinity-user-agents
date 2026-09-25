package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductIdScale
import kotlinx.serialization.Serializable

@Serializable
class SsoProductCallerScale(
    val productId: ProductIdScale,
    val executionKind: SsoProductExecutionKindScale,
)

@Serializable
enum class SsoProductExecutionKindScale {
    @EnumIndex(0)
    APP,

    @EnumIndex(1)
    WIDGET,

    @EnumIndex(2)
    WORKER,
}

fun SsoProductCallerScale.toDomain(): ProductId = ProductId.fromStoredValue(productId)

// This host never originates a product request, so the executable kind it names is the app's.
fun ProductId.toCallerScale(): SsoProductCallerScale =
    SsoProductCallerScale(productId = value, executionKind = SsoProductExecutionKindScale.APP)
