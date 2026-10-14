package io.paritytech.polkadotapp.feature_transactions_impl.data.validation

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.SerializedFallback
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.AsRawScaleValue
import io.paritytech.polkadotapp.chains.call.RuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
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

@Serializable
@SerializedFallback("Fallback")
sealed class InvalidTransaction {
    @Serializable
    data object Call : InvalidTransaction()

    @Serializable
    data object Payment : InvalidTransaction()

    @Serializable
    data object Future : InvalidTransaction()

    @Serializable
    data object Stale : InvalidTransaction()

    @Serializable
    data object BadProof : InvalidTransaction()

    @Serializable
    data object AncientBirthBlock : InvalidTransaction()

    @Serializable
    data object ExhaustsResources : InvalidTransaction()

    @Serializable
    @TransientStruct
    data class Custom(val code: UByte) : InvalidTransaction()

    @Serializable
    data object BadMandatory : InvalidTransaction()

    @Serializable
    data object MandatoryValidation : InvalidTransaction()

    @Serializable
    data object BadSigner : InvalidTransaction()

    // Absorbs any future runtime variant this client doesn't know about, so decoding never crashes.
    @Serializable
    data object Fallback : InvalidTransaction()
}

@Serializable
@SerializedFallback("Fallback")
sealed class UnknownTransaction {
    @Serializable
    data object CannotLookup : UnknownTransaction()

    @Serializable
    data object NoUnsignedValidationFunction : UnknownTransaction()

    @Serializable
    @TransientStruct
    data class Custom(val code: UByte) : UnknownTransaction()

    @Serializable
    data object Fallback : UnknownTransaction()
}
