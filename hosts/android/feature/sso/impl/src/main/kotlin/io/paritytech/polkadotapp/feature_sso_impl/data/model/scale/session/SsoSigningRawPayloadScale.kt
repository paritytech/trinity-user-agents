package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductAccountIdScale
import kotlinx.serialization.Serializable

@Serializable
class SsoSigningRawPayloadScale(
    val account: ProductAccountIdScale,
    val type: SsoPayloadTypeScale,
)

@Serializable
sealed class SsoPayloadTypeScale {
    @Serializable
    @EnumIndex(0)
    class Bytes(val data: ByteArray) : SsoPayloadTypeScale()

    @Serializable
    @EnumIndex(1)
    class Payload(val data: String) : SsoPayloadTypeScale()
}
