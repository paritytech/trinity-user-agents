package io.paritytech.polkadotapp.feature_transactions.api.data.retry

/**
 * Why an extrinsic submission needs recovery — passed to
 * [ExtrinsicSubmissionFailureRecoveryStrategy.recoverSubmissionFailure].
 */
sealed interface ExtrinsicSubmissionFailure {
    /** Pre-submission validation reported the extrinsic would be rejected; it was never submitted. */
    data object PreSubmissionValidation : ExtrinsicSubmissionFailure

    /** The submission RPC itself failed (network error, immediate rejection by the node). */
    data class Submission(val error: Throwable) : ExtrinsicSubmissionFailure

    /** The transaction pool reported the extrinsic invalid after it had been broadcast. */
    data object TxInvalidation : ExtrinsicSubmissionFailure

    /** The pool evicted the extrinsic. The bytes stay valid, so resubmitting them can still work. */
    data object PoolEviction : ExtrinsicSubmissionFailure

    /** Another extrinsic took this one's (sender, nonce). Resubmitting the same bytes cannot succeed. */
    data class Usurped(val by: String) : ExtrinsicSubmissionFailure
}
