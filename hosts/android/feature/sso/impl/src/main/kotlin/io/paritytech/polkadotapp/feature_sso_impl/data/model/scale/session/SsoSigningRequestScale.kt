package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

@Serializable
sealed class SsoSigningRequestScale {
    @Serializable
    @EnumIndex(0)
    class Transaction(val payload: SsoSignerPayloadJsonScale) : SsoSigningRequestScale()

    @Serializable
    @EnumIndex(1)
    class RawPayload(val payload: SsoSigningRawPayloadScale) : SsoSigningRequestScale()
}
