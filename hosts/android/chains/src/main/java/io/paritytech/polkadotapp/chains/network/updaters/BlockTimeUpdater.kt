package io.paritytech.polkadotapp.chains.network.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.updaters.scope.GlobalScopeUpdater
import io.paritytech.polkadotapp.chains.storage.SampledBlockTimeStorage
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.api.decodeNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.api.queryNonNull
import io.paritytech.polkadotapp.chains.storage.typed.now
import io.paritytech.polkadotapp.chains.storage.typed.number
import io.paritytech.polkadotapp.chains.storage.typed.system
import io.paritytech.polkadotapp.chains.storage.typed.timestamp
import io.paritytech.polkadotapp.common.utils.zipWithPrevious
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import timber.log.Timber
import java.math.BigInteger

data class SampledBlockTime(
    val sampleSize: BigInteger,
    val averageBlockTime: BigInteger,
)

private data class BlockTimeUpdate(
    val at: BlockHash,
    val blockNumber: BlockNumber,
    val timestamp: BigInteger,
)

class BlockTimeUpdater(
    private val chainRegistry: ChainRegistry,
    private val sampledBlockTimeStorage: SampledBlockTimeStorage,
    private val remoteStorageSource: StorageDataSource,
) : GlobalScopeUpdater {
    override suspend fun listenForUpdates(scopeValue: Unit, context: Updater.Context): Flow<Updater.SideEffect> {
        val chainId = context.chain.id

        return chainRegistry.withRuntime(chainId) {
            val blockNumberKey = runtime.metadata.system.number.storageKey()

            context.storageSubscriptionBuilder.subscribe(blockNumberKey)
                .drop(1) // ignore fist subscription value since it comes immediately
                .map { blockSubscription ->
                    val timestamp = remoteStorageSource.query(chainId, at = blockSubscription.block) {
                        runtime.metadata.timestamp.now.queryNonNull()
                    }

                    val blockNumber = runtime.metadata.system.number.decodeNonNull(blockSubscription.value)

                    BlockTimeUpdate(at = blockSubscription.block, blockNumber = blockNumber, timestamp = timestamp)
                }
                .zipWithPrevious()
                .filter { (previous, current) ->
                    previous != null && current.blockNumber - previous.blockNumber == BlockNumber.ONE
                }
                .onEach { (previousUpdate, currentUpdate) ->
                    val blockTime = currentUpdate.timestamp - previousUpdate!!.timestamp

                    updateSampledBlockTime(context.chain, blockTime)
                }.noSideAffects()
        }
    }

    private suspend fun updateSampledBlockTime(chain: Chain, newSampledTime: BigInteger) {
        val current = sampledBlockTimeStorage.get(chain.id)

        val adjustedSampleSize = current.sampleSize + BigInteger.ONE
        val adjustedAverage = (current.averageBlockTime * current.sampleSize + newSampledTime) / adjustedSampleSize
        val adjustedSampledBlockTime = SampledBlockTime(
            sampleSize = adjustedSampleSize,
            averageBlockTime = adjustedAverage
        )

        Timber.d("New block time update for chain ${chain.name}: $newSampledTime, adjustedAverage: $adjustedSampledBlockTime")

        sampledBlockTimeStorage.put(chain.id, adjustedSampledBlockTime)
    }
}
