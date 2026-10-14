package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.utils.atLeastZero

object HydraDxMathConversions {
    fun String.fromBridgeResultToBalance(): Balance? {
        return if (this == "-1") null else toBigInteger().atLeastZero().intoBalance()
    }
}
