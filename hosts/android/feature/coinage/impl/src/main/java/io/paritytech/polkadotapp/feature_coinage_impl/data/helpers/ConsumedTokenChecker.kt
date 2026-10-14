package io.paritytech.polkadotapp.feature_coinage_impl.data.helpers

import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinage
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.consumedFreeUnloadTokens
import javax.inject.Inject

interface ConsumedTokenChecker {
    data class Query(
        val period: Long,
        val alias: DataByteArray
    )

    suspend fun getNotUsedCounterIndices(
        chainId: ChainId,
        queries: List<Query>
    ): Result<List<Long>>
}

class RealConsumedTokenChecker @Inject constructor(
    @RemoteSourceQualifier private val storageDataSource: StorageDataSource,
) : ConsumedTokenChecker {
    override suspend fun getNotUsedCounterIndices(
        chainId: ChainId,
        queries: List<ConsumedTokenChecker.Query>
    ): Result<List<Long>> {
        if (queries.isEmpty()) return Result.success(emptyList())

        return storageDataSource.queryCatching(chainId) {
            val storage = runtime.metadata.coinage.consumedFreeUnloadTokens

            val entries = storage.entries(queries.map { it.toKey() })

            // TODO COINAGE: Replace with set differences
            queries.indices
                .filter { !entries.containsKey(queries[it].toKey()) }
                .map { it.toLong() }
        }
    }

    private fun ConsumedTokenChecker.Query.toKey() = period.toBigInteger() to alias
}
