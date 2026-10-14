package io.paritytech.polkadotapp.feature_transactions_impl.data.tracked

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.dao.TrackedExtrinsicDao
import io.paritytech.polkadotapp.database.model.TrackedExtrinsicLocal
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.ActiveTrackedExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.ExtrinsicTag
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.TrackedExtrinsicStatus
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/**
 * Persistence boundary for tracked extrinsics: the only place that touches [TrackedExtrinsicDao] and the
 * row⇄domain mapping. Callers work in domain terms ([TrackedExtrinsicStatus], [ActiveTrackedExtrinsic]).
 */
class TrackedExtrinsicRepository @Inject constructor(
    private val trackedExtrinsicDao: TrackedExtrinsicDao,
) {
    suspend fun save(
        tag: ExtrinsicTag,
        chainId: ChainId,
        signedExtrinsic: String,
        additional: DataByteArray?,
        createdAt: Long,
    ) {
        val pending = TrackedExtrinsicStatus.Pending.toPersisted()
        trackedExtrinsicDao.upsert(
            TrackedExtrinsicLocal(
                tag = tag.value,
                chainId = chainId,
                signedExtrinsic = signedExtrinsic,
                status = pending.status,
                blockHash = pending.blockHash,
                errorMessage = pending.errorMessage,
                additional = additional?.value,
                createdAt = createdAt,
            )
        )
    }

    suspend fun updateStatus(tag: ExtrinsicTag, status: TrackedExtrinsicStatus) {
        val persisted = status.toPersisted()
        trackedExtrinsicDao.updateStatus(tag.value, persisted.status, persisted.blockHash, persisted.errorMessage)
    }

    fun observeStatus(tag: ExtrinsicTag): Flow<TrackedExtrinsicStatus?> {
        return trackedExtrinsicDao.observe(tag.value).map { it?.toStatus() }
    }

    suspend fun getUnfinished(): List<TrackedExtrinsic> {
        return trackedExtrinsicDao.getUnfinished().map(::toDomain)
    }

    suspend fun getLatestActive(prefix: String): ActiveTrackedExtrinsic? {
        return trackedExtrinsicDao.getLatestActiveByPrefix(prefix)?.toActive()
    }

    fun observeLatestActive(prefix: String): Flow<ActiveTrackedExtrinsic?> {
        return trackedExtrinsicDao.observeLatestActiveByPrefix(prefix).map { it?.toActive() }
    }

    private fun toDomain(row: TrackedExtrinsicLocal): TrackedExtrinsic {
        return TrackedExtrinsic(ExtrinsicTag(row.tag), row.chainId, row.signedExtrinsic)
    }

    private fun TrackedExtrinsicLocal.toActive(): ActiveTrackedExtrinsic {
        return ActiveTrackedExtrinsic(ExtrinsicTag(tag), additional?.toDataByteArray())
    }

    private fun TrackedExtrinsicLocal.toStatus(): TrackedExtrinsicStatus {
        return trackedExtrinsicStatusFrom(status, blockHash, errorMessage)
    }
}
