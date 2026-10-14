package io.paritytech.polkadotapp.feature_transactions.api.data.retry

interface ExtrinsicSubmissionFailureRecoveryStrategy {
    /**
     * Decides what to do about [failure]. [context] is the only capability granted: ask the chain, do not send.
     */
    suspend fun recoverSubmissionFailure(
        context: ExtrinsicRecoveryContext,
        failure: ExtrinsicSubmissionFailure,
    ): ExtrinsicSubmissionFailureRecovery
}

object Abort : ExtrinsicSubmissionFailureRecoveryStrategy {
    override suspend fun recoverSubmissionFailure(
        context: ExtrinsicRecoveryContext,
        failure: ExtrinsicSubmissionFailure,
    ): ExtrinsicSubmissionFailureRecovery {
        return ExtrinsicSubmissionFailureRecovery.Abort
    }
}
