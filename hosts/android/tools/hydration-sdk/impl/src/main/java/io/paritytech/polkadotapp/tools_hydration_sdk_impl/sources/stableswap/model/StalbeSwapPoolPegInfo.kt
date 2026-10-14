package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.common.data.substrate.castToStruct

class StalbeSwapPoolPegInfo(
    val current: List<List<Balance>>
)

fun bindPoolPegInfo(decoded: Any?): StalbeSwapPoolPegInfo {
    val asStruct = decoded.castToStruct()
    return StalbeSwapPoolPegInfo(
        current = bindList(asStruct["current"]) { item ->
            bindList(item, ::bindBalance)
        }
    )
}
