package io.paritytech.polkadotapp.feature_xcm_api.weight

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.common.utils.scale.ToDynamicScaleInstance
import kotlinx.serialization.Serializable

@Serializable
sealed class WeightLimit : ToDynamicScaleInstance {
    override fun toEncodableInstance(): Any? {
        return Scale.encode(this)
    }

    companion object {
        fun zero(): WeightLimit {
            return Limited(WeightV2.zero())
        }
    }

    @Serializable
    data object Unlimited : WeightLimit()

    @Serializable
    @TransientStruct
    data class Limited(val weightV2: WeightV2) : WeightLimit()
}
