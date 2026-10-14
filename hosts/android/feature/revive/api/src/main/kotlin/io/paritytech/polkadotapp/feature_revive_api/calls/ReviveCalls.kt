package io.paritytech.polkadotapp.feature_revive_api.calls

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.metadata.call
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.call
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_revive_api.EvmAccountId
import java.math.BigInteger

@JvmInline
value class ReviveCalls(val extrinsicBuilder: ExtrinsicBuilder)

val ExtrinsicBuilder.revive: ReviveCalls
    get() = ReviveCalls(this)

fun ReviveCalls.call(
    dest: EvmAccountId,
    weightLimit: WeightV2,
    storageDepositLimit: BigInteger,
    data: DataByteArray,
) {
    extrinsicBuilder.call(
        moduleName = Modules.REVIVE,
        callName = "call",
        arguments = autoEncodedArgs(
            "dest" to dest,
            "value" to BigInteger.ZERO,
            weightLimitArgumentName() to weightLimit,
            "storage_deposit_limit" to storageDepositLimit,
            "data" to data,
        ),
    )
}

// pallet-revive renamed `gas_limit` to `weight_limit` once `gas` came to mean Ethereum gas; both runtimes are live.
private fun ReviveCalls.weightLimitArgumentName(): String {
    val arguments = extrinsicBuilder.runtime.metadata.module(Modules.REVIVE).call("call").arguments

    return if (arguments.any { it.name == "weight_limit" }) "weight_limit" else "gas_limit"
}
