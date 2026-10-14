package io.paritytech.polkadotapp.feature_transactions_impl.data.retry

import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicRecoveryContext
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecovery
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecoveryStrategy

/**
 * Routes recovery by failure cause: a post-pool [ExtrinsicSubmissionFailure.TxInvalidation] (the
 * fork/reorg case) or a [ExtrinsicSubmissionFailure.PoolEviction] is handed to [onTxInvalidation]
 * (typically [ResubmitWhenValid]) because the same bytes can still be included; a genuine
 * pre-submission rejection, a transport-level submission error, or a usurped nonce aborts immediately
 * so the caller sees the failure fast instead of waiting out the mortality window.
 */
class CauseBasedRecoveryStrategy(
    private val onTxInvalidation: ExtrinsicSubmissionFailureRecoveryStrategy,
) : ExtrinsicSubmissionFailureRecoveryStrategy {
    override suspend fun recoverSubmissionFailure(
        context: ExtrinsicRecoveryContext,
        failure: ExtrinsicSubmissionFailure,
    ): ExtrinsicSubmissionFailureRecovery = when (failure) {
        ExtrinsicSubmissionFailure.TxInvalidation,
        ExtrinsicSubmissionFailure.PoolEviction -> onTxInvalidation.recoverSubmissionFailure(context, failure)
        ExtrinsicSubmissionFailure.PreSubmissionValidation,
        is ExtrinsicSubmissionFailure.Usurped,
        is ExtrinsicSubmissionFailure.Submission -> ExtrinsicSubmissionFailureRecovery.Abort
    }
}
