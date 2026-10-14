package io.paritytech.polkadotapp.feature_coinage_impl.data.model

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import kotlinx.serialization.Serializable

@Keep
@Serializable
class OnChainInstanceRecord(
    val assetUnit: BigIntegerSerializable
)
