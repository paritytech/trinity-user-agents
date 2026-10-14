package io.paritytech.polkadotapp.feature_coinage_impl.data.model

import androidx.annotation.Keep
import kotlinx.serialization.Serializable

@Keep
@Serializable
class OnChainCoinInfo(
    val instanceId: Int,
    val value: Int,
    val age: Int
)
