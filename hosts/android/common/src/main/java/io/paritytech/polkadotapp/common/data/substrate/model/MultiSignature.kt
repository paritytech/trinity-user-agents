package io.paritytech.polkadotapp.common.data.substrate.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import kotlinx.serialization.Serializable

@Serializable
sealed class MultiSignature {
    @Serializable
    @TransientStruct
    class Sr25519(val signature: DataByteArray) : MultiSignature()
}
