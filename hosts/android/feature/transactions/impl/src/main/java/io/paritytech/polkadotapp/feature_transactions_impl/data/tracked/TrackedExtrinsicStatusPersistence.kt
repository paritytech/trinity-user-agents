package io.paritytech.polkadotapp.feature_transactions_impl.data.tracked

import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.TrackedExtrinsicStatus

/**
 * Persisted form of [TrackedExtrinsicStatus]: a status column plus the optional `blockHash`/`errorMessage`
 * payloads. The terminal column values (`FINALIZED`, `FAILED`) match the `NOT IN (...)` filter the DAO uses
 * to select still-watched rows, so this mapping and the DAO query must stay in sync.
 */
internal object TrackedExtrinsicStatusColumn {
    const val PENDING = "PENDING"
    const val ACCEPTED = "ACCEPTED"
    const val IN_BLOCK = "IN_BLOCK"
    const val FINALIZED = "FINALIZED"
    const val FAILED = "FAILED"
}

internal class PersistedStatus(val status: String, val blockHash: String?, val errorMessage: String?)

internal fun TrackedExtrinsicStatus.toPersisted(): PersistedStatus = when (this) {
    TrackedExtrinsicStatus.Pending -> PersistedStatus(TrackedExtrinsicStatusColumn.PENDING, null, null)
    TrackedExtrinsicStatus.Accepted -> PersistedStatus(TrackedExtrinsicStatusColumn.ACCEPTED, null, null)
    is TrackedExtrinsicStatus.InBlock -> PersistedStatus(TrackedExtrinsicStatusColumn.IN_BLOCK, blockHash, null)
    is TrackedExtrinsicStatus.Finalized -> PersistedStatus(TrackedExtrinsicStatusColumn.FINALIZED, blockHash, null)
    is TrackedExtrinsicStatus.Failed -> PersistedStatus(TrackedExtrinsicStatusColumn.FAILED, null, message)
}

internal fun trackedExtrinsicStatusFrom(status: String, blockHash: String?, errorMessage: String?): TrackedExtrinsicStatus =
    when (status) {
        TrackedExtrinsicStatusColumn.PENDING -> TrackedExtrinsicStatus.Pending
        TrackedExtrinsicStatusColumn.ACCEPTED -> TrackedExtrinsicStatus.Accepted
        TrackedExtrinsicStatusColumn.IN_BLOCK ->
            TrackedExtrinsicStatus.InBlock(requireNotNull(blockHash) { "IN_BLOCK row is missing a block hash" })
        TrackedExtrinsicStatusColumn.FINALIZED ->
            TrackedExtrinsicStatus.Finalized(requireNotNull(blockHash) { "FINALIZED row is missing a block hash" })
        TrackedExtrinsicStatusColumn.FAILED ->
            TrackedExtrinsicStatus.Failed(requireNotNull(errorMessage) { "FAILED row is missing an error message" })
        else -> error("Unknown tracked extrinsic status column: $status")
    }
