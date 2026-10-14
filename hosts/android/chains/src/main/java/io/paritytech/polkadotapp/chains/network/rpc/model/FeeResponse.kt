package io.paritytech.polkadotapp.chains.network.rpc.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import kotlinx.serialization.Serializable

@Serializable
class FeeResponse(
    val partialFee: Balance,
    val weight: WeightV2,
)
