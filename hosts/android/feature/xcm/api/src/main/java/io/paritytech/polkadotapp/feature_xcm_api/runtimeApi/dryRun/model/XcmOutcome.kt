package io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.RawScaleValue
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import kotlinx.serialization.Serializable

@Serializable
sealed class XcmOutcome {
    companion object {
        fun bind(decodedInstance: Any?): XcmOutcome {
            return Scale.decode(decodedInstance)
        }
    }

    @Serializable
    class Complete(val used: WeightV2) : XcmOutcome()

    @Serializable
    class Incomplete(val used: WeightV2, val error: RawScaleValue) : XcmOutcome()

    @Serializable
    class Error(val error: RawScaleValue) : XcmOutcome()
}
