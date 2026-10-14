package io.paritytech.polkadotapp.feature_transactions.api.data

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.Era
import io.novasama.substrate_sdk_android.runtime.extrinsic.BatchMode
import io.novasama.substrate_sdk_android.runtime.extrinsic.ExtrinsicVersion
import io.novasama.substrate_sdk_android.runtime.extrinsic.Nonce
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePayment
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.NativeFeePayment
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.Abort
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecoveryStrategy
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.ExtrinsicSubmission
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import kotlinx.coroutines.flow.Flow

typealias FormExtrinsic = suspend context(WithRuntime) ExtrinsicBuilder.() -> Unit
typealias FormMultiExtrinsic = suspend MultiExtrinsicBuilder.() -> Unit

class FormExtrinsicWithOrigin(
    val formExtrinsic: FormExtrinsic,
    val origin: TransactionOrigin
)

/**
 * [eraBlockNumber] is the number of the block [blockHash] identifies. It is null when the mortality was
 * recovered from an already-signed payload, which carries the anchor's hash but not its number.
 */
class Mortality(val era: Era, val blockHash: DataByteArray, val eraBlockNumber: Long?) {
    companion object {
        fun immortal(chain: Chain): Mortality {
            return Mortality(
                era = Era.Immortal,
                blockHash = chain.genesisHash,
                eraBlockNumber = 0
            )
        }

        fun externallyPassed(era: Era, blockHash: DataByteArray): Mortality {
            return Mortality(era, blockHash, eraBlockNumber = null)
        }
    }
}

interface ExtrinsicService {
    data class SubmissionOptions(
        val batchMode: BatchMode = BatchMode.BATCH_ALL,
        val feePayment: FeePayment = NativeFeePayment(),
        val extrinsicVersion: ExtrinsicVersion? = null,
        // Well-known extensions that ExtrinsicService will fill automatically when not specified manually
        val mortality: Mortality? = null,
        val nonce: Nonce? = null,
        val tip: Balance = Balance.ZERO,
        val metadataHash: DataByteArray? = null,
        val transactionVersion: Int? = null,
        val specVersion: Int? = null,
    )

    suspend fun submitExtrinsic(
        chain: Chain,
        origin: TransactionOrigin,
        options: SubmissionOptions = SubmissionOptions(),
        formExtrinsic: FormExtrinsic,
    ): Result<ExtrinsicSubmission>

    suspend fun submitExtrinsicAndAwaitExecution(
        chain: Chain,
        origin: TransactionOrigin,
        options: SubmissionOptions = SubmissionOptions(),
        // null resolves to the default recovery (ResubmitWhenValid on TxInvalidation). Pass Abort to fail fast.
        submissionFailureRecovery: ExtrinsicSubmissionFailureRecoveryStrategy? = null,
        formExtrinsic: FormExtrinsic,
    ): Result<ExtrinsicExecutionResult>

    suspend fun submitExtrinsicsAndAwaitInBlock(
        chain: Chain,
        options: SubmissionOptions = SubmissionOptions(),
        submissionFailureRecovery: ExtrinsicSubmissionFailureRecoveryStrategy = Abort,
        formExtrinsic: FormMultiExtrinsic,
    ): Result<List<Result<ExtrinsicStatus.InBlock>>>

    suspend fun estimateFee(
        chain: Chain,
        origin: TransactionOrigin,
        options: SubmissionOptions = SubmissionOptions(),
        formExtrinsic: FormExtrinsic,
    ): Result<AccountFee>

    suspend fun buildExtrinsic(
        chain: Chain,
        origin: TransactionOrigin,
        options: SubmissionOptions,
        formExtrinsic: FormExtrinsic,
    ): Result<EnrichedSendableExtrinsic>

    /**
     * Builds and signs every extrinsic of [formExtrinsic] up front, with nonces sequenced across the batch,
     * and submits none of them. For callers that must durably record each extrinsic before its bytes reach
     * the wire: build here, record, then submit each via [submitAndWatchBuiltExtrinsic].
     */
    suspend fun buildExtrinsics(
        chain: Chain,
        options: SubmissionOptions = SubmissionOptions(),
        formExtrinsic: FormMultiExtrinsic,
    ): Result<List<EnrichedSendableExtrinsic>>

    /**
     * Submits and watches an already-built [extrinsic] through to [ExtrinsicStatus.Finalized], applying
     * [submissionFailureRecovery] on submission failures. Unlike the form-based overloads it does not
     * rebuild/re-sign, so callers (e.g. background watchers resuming from a persisted hex) keep the exact
     * same nonce and mortality across resubmissions.
     */
    fun submitAndWatchBuiltExtrinsic(
        chain: Chain,
        extrinsic: SendableExtrinsic,
        submissionFailureRecovery: ExtrinsicSubmissionFailureRecoveryStrategy,
    ): Flow<ExtrinsicStatus>
}
