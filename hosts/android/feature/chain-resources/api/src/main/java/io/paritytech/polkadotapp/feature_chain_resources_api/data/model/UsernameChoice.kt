package io.paritytech.polkadotapp.feature_chain_resources_api.data.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import kotlinx.serialization.Serializable

@Serializable
sealed class UsernameChoice {
    @Serializable
    @TransientStruct
    class Reservation(val username: String) : UsernameChoice()

    @Serializable
    @TransientStruct
    class Standalone(val username: String) : UsernameChoice()
}
