package io.paritytech.polkadotapp.feature_revive_impl.data

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.paritytech.polkadotapp.chains.network.binding.DynamicDispatchError
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Minimal models for the Revive pallet `ContractResult`: the dynamic SCALE format ignores fields not declared here.
@Serializable
class ReviveContractResult(
    val result: ScaleResult<ExecReturnValue, DynamicDispatchError>
)

@Serializable
class ReviveDryRunResult(
    val weightRequired: WeightV2,
    val storageDeposit: ReviveStorageDeposit,
    val result: ScaleResult<ExecReturnValue, DynamicDispatchError>,
)

@Serializable
sealed interface ReviveStorageDeposit {
    @Serializable
    @SerialName("Refund")
    @TransientStruct
    class Refund(val value: BigIntegerSerializable) : ReviveStorageDeposit

    @Serializable
    @SerialName("Charge")
    @TransientStruct
    class Charge(val value: BigIntegerSerializable) : ReviveStorageDeposit
}

@Serializable
class ExecReturnValue(
    val flags: ReturnFlags,
    val data: DataByteArray,
) {
    val isReverted: Boolean
        get() = flags.bits and ReturnFlags.REVERT != 0
}

@Serializable
class ReturnFlags(val bits: Int) {
    companion object {
        const val REVERT = 0x1
    }
}
