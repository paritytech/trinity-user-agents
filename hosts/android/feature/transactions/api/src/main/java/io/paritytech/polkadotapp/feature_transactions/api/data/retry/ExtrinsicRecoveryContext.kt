package io.paritytech.polkadotapp.feature_transactions.api.data.retry

/**
 * What a recovery strategy is handed instead of the extrinsic itself: a way to ask whether the failed
 * extrinsic would be accepted now, and nothing else.
 *
 * Deliberately not the bytes. A strategy holding a `SendableExtrinsic` could submit it, and then an extrinsic
 * would be on the wire that the service never saw — which would quietly break the one thing callers read a
 * pre-submission rejection as proving: that nothing was ever sent.
 */
interface ExtrinsicRecoveryContext {
    /** Re-validates the failed extrinsic against the chain's current head. */
    suspend fun revalidate(): Result<ExtrinsicRecoveryVerdict>
}

/** The runtime's answer, reduced to what a recovery decision actually turns on. */
enum class ExtrinsicRecoveryVerdict {
    /** It would be accepted now, so resubmitting the same bytes can work. */
    ACCEPTED,

    /** Rejected now but not for good — a later block may accept it. */
    REJECTED_FOR_NOW,

    /** Rejected permanently: its mortality window has passed and no wait can help. */
    EXPIRED,

    /** The runtime could not say; treated like [REJECTED_FOR_NOW] by anything that waits. */
    UNKNOWN,
}
