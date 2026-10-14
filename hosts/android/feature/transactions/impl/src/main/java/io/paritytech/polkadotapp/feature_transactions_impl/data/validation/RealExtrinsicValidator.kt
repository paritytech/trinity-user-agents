package io.paritytech.polkadotapp.feature_transactions_impl.data.validation

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.RawScaleValue
import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import javax.inject.Inject

class RealExtrinsicValidator @Inject constructor(
    private val multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi,
) : ExtrinsicValidator {
    override suspend fun validate(
        chainId: ChainId,
        extrinsic: EncodedExtrinsicBody,
        atBlockHash: BlockHash,
    ): Result<TransactionValidity> = runCancellableCatching {
        multiChainRuntimeCallsApi.forChain(chainId)
            .validateTransaction(
                source = TransactionSource.External,
                extrinsic = extrinsic,
                atBlockHash = atBlockHash,
            )
            .toTransactionValidity()
    }
}

internal fun ScaleResult<RawScaleValue, TransactionValidityError>.toTransactionValidity(): TransactionValidity {
    return when (this) {
        is ScaleResult.Ok -> TransactionValidity.Valid
        is ScaleResult.Error -> when (val validityError = error) {
            is TransactionValidityError.Invalid -> TransactionValidity
                .Invalid(validityError.reason)
            is TransactionValidityError.Unknown -> TransactionValidity.Unknown(validityError.reason)
        }
    }
}
