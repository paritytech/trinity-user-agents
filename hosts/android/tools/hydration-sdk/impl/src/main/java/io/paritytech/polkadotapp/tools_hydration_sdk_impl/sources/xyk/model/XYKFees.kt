package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.model

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.network.binding.bindInt
import io.paritytech.polkadotapp.chains.util.constant
import io.paritytech.polkadotapp.chains.util.decoded
import io.paritytech.polkadotapp.common.data.substrate.castToList

class XYKFees(val nominator: Int, val denominator: Int)

fun bindXYKFees(decoded: Any?): XYKFees {
    val (first, second) = decoded.castToList()

    return XYKFees(bindInt(first), bindInt(second))
}

fun Module.poolFeesConstant(runtimeSnapshot: RuntimeSnapshot): XYKFees {
    return bindXYKFees(constant("GetExchangeFee").decoded(runtimeSnapshot))
}
