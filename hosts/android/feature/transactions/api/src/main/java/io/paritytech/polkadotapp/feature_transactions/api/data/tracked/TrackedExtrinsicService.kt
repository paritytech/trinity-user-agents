package io.paritytech.polkadotapp.feature_transactions.api.data.tracked

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.FormExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first

interface TrackedExtrinsicService {
    suspend fun submit(
        tag: ExtrinsicTag,
        chain: Chain,
        origin: TransactionOrigin,
        options: ExtrinsicService.SubmissionOptions = ExtrinsicService.SubmissionOptions(),
        additional: DataByteArray?,
        formExtrinsic: FormExtrinsic,
    ): Result<Unit>

    fun observeStatus(tag: ExtrinsicTag): Flow<TrackedExtrinsicStatus?>

    /** Most recent non-terminal tracked extrinsic whose tag starts with [prefix], or null if none is in flight. */
    suspend fun getLatestActive(prefix: String): ActiveTrackedExtrinsic?

    /** Most recent non-terminal tracked extrinsic whose tag starts with [prefix]; emits null when none is in flight. */
    fun observeLatestActive(prefix: String): Flow<ActiveTrackedExtrinsic?>
}

class TrackedExtrinsicFailed(message: String) : Exception(message)

/**
 * Suspends until the node accepts [tag] into the pool (or it settles), then returns. Acceptance — not in-block —
 * is the release point: in-block dispatch errors surface later, not here. A terminal `Failed` maps to a failure.
 */
suspend fun TrackedExtrinsicService.awaitAcceptance(tag: ExtrinsicTag): Result<Unit> {
    val status = observeStatus(tag)
        .filterNotNull()
        .first { it != TrackedExtrinsicStatus.Pending }

    return when (status) {
        is TrackedExtrinsicStatus.Failed -> Result.failure(TrackedExtrinsicFailed(status.message))
        else -> Result.success(Unit)
    }
}

suspend fun TrackedExtrinsicService.getLatestActive(prefix: ExtrinsicTag): ActiveTrackedExtrinsic? {
    return getLatestActive(prefix.value)
}

fun TrackedExtrinsicService.observeLatestActive(prefix: ExtrinsicTag): Flow<ActiveTrackedExtrinsic?> {
    return observeLatestActive(prefix.value)
}
