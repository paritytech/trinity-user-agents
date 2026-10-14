package io.paritytech.polkadotapp.feature_transactions_impl.data.retry

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicRecoveryContext
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicRecoveryVerdict
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecovery
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecoveryStrategy
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.withIndex
import timber.log.Timber

/**
 * Fork-protection recovery strategy: re-validates the failed extrinsic against the chain head on every new
 * block and asks for a resubmission once the runtime accepts it. Bounded by the extrinsic's mortality (it
 * eventually validates as expired) and, when set, by [maxAttempts].
 */
class ResubmitWhenValid(
    private val chainId: ChainId,
    private val maxAttempts: Int?,
    private val chainStateRepository: ChainStateRepository,
) : ExtrinsicSubmissionFailureRecoveryStrategy {
    override suspend fun recoverSubmissionFailure(
        context: ExtrinsicRecoveryContext,
        failure: ExtrinsicSubmissionFailure,
    ): ExtrinsicSubmissionFailureRecovery {
        Timber.i("Submission failed on $chainId ($failure) — starting validity-driven retry")

        return chainStateRepository.currentRemoteBlockNumberFlow(chainId, sharedRequestsBuilder = null)
            .withIndex()
            .mapNotNull { (attempt, _) -> decideRecovery(context, attempt) }
            .first()
    }

    private suspend fun decideRecovery(
        context: ExtrinsicRecoveryContext,
        attempt: Int,
    ): ExtrinsicSubmissionFailureRecovery? {
        if (maxAttempts != null && attempt >= maxAttempts) {
            Timber.i("Reached maxAttempts ($maxAttempts) on $chainId — aborting")
            return ExtrinsicSubmissionFailureRecovery.Abort
        }

        val verdict = context.revalidate().getOrNull()
        Timber.d("Re-validated extrinsic on $chainId (attempt $attempt): $verdict")

        return when (verdict) {
            ExtrinsicRecoveryVerdict.ACCEPTED -> {
                Timber.i("Extrinsic valid again on $chainId — resubmitting")
                ExtrinsicSubmissionFailureRecovery.Resubmit
            }

            ExtrinsicRecoveryVerdict.EXPIRED -> {
                Timber.i("Extrinsic mortality expired on $chainId — aborting")
                ExtrinsicSubmissionFailureRecovery.Abort
            }

            // Still rejected, or the runtime could not say — keep waiting for the next block.
            ExtrinsicRecoveryVerdict.REJECTED_FOR_NOW, ExtrinsicRecoveryVerdict.UNKNOWN, null -> null
        }
    }
}
