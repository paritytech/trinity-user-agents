package io.paritytech.polkadotapp.feature_coinage_impl.data.model

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import kotlinx.serialization.Serializable

@Keep
@Serializable
@AsTuple
class OnChainRecyclerLocation(
    val instanceId: Int,
    val value: Int
)
