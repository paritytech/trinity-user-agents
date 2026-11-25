package io.paritytech.polkadotapp.feature_transactions_impl.data.validation

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.AsRawScaleValue
import io.paritytech.polkadotapp.chains.call.RuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.EncodedExtrinsicBody
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.InvalidTransaction
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.UnknownTransaction
import kotlinx.serialization.Serializable

private const val SECTION = "TaggedTransactionQueue"
private const val METHOD = "validate_transaction"

/**
 * Calls `TaggedTransactionQueue_validate_transaction` to check whether [extrinsic] would currently be
 * accepted by the transaction pool, evaluated against the runtime state at [atBlockHash].
 */
suspend fun RuntimeCallsApi.validateTransaction(
    source: TransactionSource,
    extrinsic: EncodedExtrinsicBody,
    atBlockHash: BlockHash,
): ScaleResult<AsRawScaleValue, TransactionValidityError> {
    return call(
        section = SECTION,
        method = METHOD,
        arguments = autoEncodedArgs(
            "source" to source,
            // raw body bytes; autoEncodedArgs SCALE-encodes them as Vec<u8> — the `tx` wire format
            "tx" to extrinsic,
            "block_hash" to atBlockHash.hexToDataByteArray(),
        ),
    )
}

@Serializable
sealed interface TransactionSource {
    @Serializable
    data object InBlock : TransactionSource

    @Serializable
    data object Local : TransactionSource

    @Serializable
    data object External : TransactionSource
}

@Serializable
sealed class TransactionValidityError {
    @Serializable
    @TransientStruct
    data class Invalid(val reason: InvalidTransaction) : TransactionValidityError()

    @Serializable
    @TransientStruct
    data class Unknown(val reason: UnknownTransaction) : TransactionValidityError()
}
