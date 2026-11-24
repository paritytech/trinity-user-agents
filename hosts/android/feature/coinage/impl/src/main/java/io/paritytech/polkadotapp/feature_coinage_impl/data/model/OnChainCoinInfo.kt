package io.paritytech.polkadotapp.feature_coinage_impl.data.model

import androidx.annotation.Keep
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstanceId
import kotlinx.serialization.Serializable

@Keep
@Serializable
class OnChainCoinInfo(
    val instanceId: Int,
    val value: Int,
    val age: Int
)

fun OnChainCoinInfo.belongsTo(instance: CoinageInstanceId): Boolean = instanceId.toUInt() == instance
