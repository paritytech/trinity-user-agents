package io.paritytech.polkadotapp.feature_transactions_impl.data.tracked

import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.TrackedExtrinsicStatus

/**
 * Maps a single watch-stream [ExtrinsicStatus] onto the persisted [TrackedExtrinsicStatus]. Pure — no IO.
 *
 * Pool acceptance (`Ready`/`Broadcast`) collapses to [TrackedExtrinsicStatus.Accepted]. An in-block extrinsic
 * whose call dispatch failed is reported as terminal `Failed` carrying [inBlockDispatchError] rather than
 * `InBlock`. `Other` yields `null` (no observable change).
 */
internal object TrackedExtrinsicStatusReducer {
    fun reduce(status: ExtrinsicStatus, inBlockDispatchError: String?): TrackedExtrinsicStatus? = when (status) {
        is ExtrinsicStatus.Ready, is ExtrinsicStatus.Broadcast -> TrackedExtrinsicStatus.Accepted

        is ExtrinsicStatus.InBlock ->
            if (inBlockDispatchError != null) {
                TrackedExtrinsicStatus.Failed(inBlockDispatchError)
            } else {
                TrackedExtrinsicStatus.InBlock(status.blockHash)
            }

        is ExtrinsicStatus.Finalized -> TrackedExtrinsicStatus.Finalized(status.blockHash)

        is ExtrinsicStatus.FailedToSubmit ->
            TrackedExtrinsicStatus.Failed(status.exception.message ?: "Failed to submit extrinsic")

        // Only reached when recovery already gave up on it: TxInvalidation is retried by ResubmitWhenValid until
        // it re-validates or mortality expires, so a terminal Invalid here means the recovery aborted it.
        is ExtrinsicStatus.Invalid -> TrackedExtrinsicStatus.Failed("Extrinsic was rejected as invalid")

        // Same: terminal here means recovery already stopped resubmitting.
        is ExtrinsicStatus.Dropped -> TrackedExtrinsicStatus.Failed("Extrinsic was dropped from the transaction pool")

        is ExtrinsicStatus.Usurped -> TrackedExtrinsicStatus.Failed("Extrinsic was usurped by ${status.by}")

        // Still in flight — no observable change for a tracked extrinsic.
        is ExtrinsicStatus.Future, is ExtrinsicStatus.Retracted, is ExtrinsicStatus.Other -> null
    }
}
