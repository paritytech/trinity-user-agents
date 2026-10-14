package io.paritytech.polkadotapp.feature_transactions.api.data.retry

sealed interface ExtrinsicSubmissionFailureRecovery {
    data object Abort : ExtrinsicSubmissionFailureRecovery

    /** Submit the same bytes again. The service holds them; a strategy is never given them to send itself. */
    data object Resubmit : ExtrinsicSubmissionFailureRecovery
}
