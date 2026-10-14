package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.network.binding.bindPermill
import io.paritytech.polkadotapp.chains.util.constant
import io.paritytech.polkadotapp.chains.util.decoded
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.common.utils.Fraction

class DynamicFee(
    val assetFee: Fraction,
    val protocolFee: Fraction
)

fun bindDynamicFee(decoded: Any): DynamicFee {
    val asStruct = decoded.castToStruct()

    return DynamicFee(
        assetFee = bindPermill(asStruct["assetFee"]),
        protocolFee = bindPermill(asStruct["protocolFee"]),
    )
}

class FeeParams(
    val minFee: Fraction,
)

fun Module.feeParamsConstant(name: String, runtimeSnapshot: RuntimeSnapshot): FeeParams {
    return bindFeeParams(constant(name).decoded(runtimeSnapshot))
}

private fun bindFeeParams(decoded: Any?): FeeParams {
    val asStruct = decoded.castToStruct()

    return FeeParams(
        minFee = bindPermill(asStruct["minFee"]),
    )
}
