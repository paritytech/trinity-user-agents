package io.paritytech.polkadotapp.feature_transactions_impl.data.validation

import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import timber.log.Timber
import javax.inject.Inject

/**
 * Pre-submission gate shared by the live submit path and the tracked-extrinsic submit path.
 *
 * The pool temp-bans an extrinsic whose submission it rejects, which then blocks resubmitting it, so a tx the
 * runtime reports as definitely [TransactionValidity.Invalid] must never be submitted. Only a definite Invalid
 * verdict blocks; an [TransactionValidity.Unknown] verdict or a failed validation call is inconclusive and is
 * allowed through rather than risk blocking a healthy extrinsic.
 */
class PreSubmissionValidator @Inject constructor(
    private val extrinsicValidator: ExtrinsicValidator,
    private val rpcCalls: RpcCalls,
) {
    /** Returns false only on a definite Invalid verdict; an Unknown verdict or a failed call is allowed through. */
    suspend fun mightBeValid(chainId: ChainId, extrinsic: SendableExtrinsic): Boolean {
        val blockHash = rpcCalls.getBlockHash(chainId)
        val extrinsicBody = extrinsic.bytesWithoutLength.toDataByteArray()
        val validityResult = extrinsicValidator.validate(chainId, extrinsicBody, blockHash)

        return when (val validity = validityResult.getOrNull()) {
            is TransactionValidity.Invalid -> {
                Timber.e("Found invalid transaction: ${validity.reason}. Blocking submission")
                false
            }

            is TransactionValidity.Valid -> true

            is TransactionValidity.Unknown -> {
                Timber.w("Got ${validity.reason}. Validity inconclusive, allowing submission")
                true
            }

            null -> {
                Timber.w(validityResult.exceptionOrNull(), "Validator failed. Inconclusive, allowing submission")
                true
            }
        }
    }
}
