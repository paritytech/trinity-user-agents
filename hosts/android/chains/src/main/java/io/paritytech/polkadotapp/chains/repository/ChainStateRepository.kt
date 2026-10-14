package io.paritytech.polkadotapp.chains.repository

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.network.updaters.SampledBlockTime
import io.paritytech.polkadotapp.chains.storage.SampledBlockTimeStorage
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.api.observeNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.api.queryNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.typed.number
import io.paritytech.polkadotapp.chains.storage.typed.system
import io.paritytech.polkadotapp.chains.util.BlockDurationEstimator
import io.paritytech.polkadotapp.chains.util.babeOrNull
import io.paritytech.polkadotapp.chains.util.isParachain
import io.paritytech.polkadotapp.chains.util.numberConstant
import io.paritytech.polkadotapp.chains.util.optionalNumberConstant
import io.paritytech.polkadotapp.chains.util.timestampOrNull
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import java.math.BigInteger
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

private const val FALLBACK_BLOCK_TIME_MILLIS_RELAYCHAIN = 6 * 1000
private const val FALLBACK_BLOCK_TIME_MILLIS_PARACHAIN = 2 * FALLBACK_BLOCK_TIME_MILLIS_RELAYCHAIN

private const val PERIOD_VALIDITY_THRESHOLD = 100

private val REQUIRED_SAMPLED_BLOCKS = 10.toBigInteger()

interface ChainStateRepository {
    suspend fun sampledBlockTimeInMillis(chainId: ChainId): BigInteger

    suspend fun expectedBlockTime(chainId: ChainId): Duration

    suspend fun currentBlock(chainId: ChainId): BlockNumber

    suspend fun currentBlockHash(chainId: ChainId): BlockHash

    fun currentBlockNumberFlow(chainId: ChainId): Flow<BlockNumber>

    suspend fun blockHashCount(chainId: ChainId): BigInteger?

    suspend fun currentRemoteBlockNumberFlow(
        chainId: ChainId,
        sharedRequestsBuilder: SharedRequestsBuilder?
    ): Flow<BlockNumber>

    suspend fun currentRemoteBlockNumber(chainId: ChainId): BlockNumber

    suspend fun getFinalizedBlockHash(chainId: ChainId): BlockHash
}

suspend fun ChainStateRepository.currentBlockHashCatching(chainId: ChainId): Result<BlockHash> = runCancellableCatching {
    currentBlockHash(chainId)
}

suspend fun ChainStateRepository.blockDurationEstimator(chainId: ChainId): BlockDurationEstimator {
    return BlockDurationEstimator(
        currentBlock = currentBlock(chainId),
        blockTimeMillis = sampledBlockTimeInMillis(chainId)
    )
}

suspend fun ChainStateRepository.blockDurationEstimatorFlow(chainId: ChainId): Flow<BlockDurationEstimator> {
    return currentBlockNumberFlow(chainId).map {
        BlockDurationEstimator(
            currentBlock = currentBlock(chainId),
            blockTimeMillis = sampledBlockTimeInMillis(chainId)
        )
    }
}

internal class RealChainStateRepository(
    private val localStorage: StorageDataSource,
    private val remoteStorage: StorageDataSource,
    private val sampledBlockTimeStorage: SampledBlockTimeStorage,
    private val chainRegistry: ChainRegistry,
    private val dispatchers: CoroutineDispatchers,
    private val rpcCalls: RpcCalls,
) : ChainStateRepository {
    override suspend fun sampledBlockTimeInMillis(chainId: ChainId): BigInteger {
        return withContext(dispatchers.io) {
            val runtime = chainRegistry.getRuntime(chainId)
            val chain = chainRegistry.getChain(chainId)

            val blockTimeFromConstants = blockTimeFromConstants(chain, runtime)
            val sampledBlockTime = sampledBlockTimeStorage.get(chainId)

            weightedAverageBlockTime(sampledBlockTime, blockTimeFromConstants)
        }
    }

    override suspend fun expectedBlockTime(chainId: ChainId): Duration {
        return withContext(dispatchers.io) {
            val runtime = chainRegistry.getRuntime(chainId)
            val chain = chainRegistry.getChain(chainId)

            blockTimeFromConstants(chain, runtime)
                .toLong().milliseconds
        }
    }

    override suspend fun currentBlock(chainId: ChainId) = localStorage.query(chainId) {
        metadata.system.number.queryNonNull()
    }

    override suspend fun currentBlockHash(chainId: ChainId): BlockHash {
        return rpcCalls.getBlockHash(chainId)
    }

    override fun currentBlockNumberFlow(chainId: ChainId): Flow<BlockNumber> = localStorage.subscribe(chainId) {
        metadata.system.number.observeNonNull()
    }

    override suspend fun blockHashCount(chainId: ChainId): BigInteger? {
        return localStorage.query(chainId) {
            metadata.system.module.optionalNumberConstant("BlockHashCount", runtime)
        }
    }

    override suspend fun currentRemoteBlockNumberFlow(
        chainId: ChainId,
        sharedRequestsBuilder: SharedRequestsBuilder?
    ): Flow<BlockNumber> {
        return if (sharedRequestsBuilder != null) {
            remoteStorage.subscribe(chainId, subscriptionBuilder = sharedRequestsBuilder) {
                metadata.system.number.observeNonNull()
            }
        } else {
            remoteStorage.subscribe(chainId) {
                metadata.system.number.observeNonNull()
            }
        }
    }

    override suspend fun currentRemoteBlockNumber(chainId: ChainId): BlockNumber {
        return remoteStorage.query(chainId) {
            metadata.system.number.queryNonNull()
        }
    }

    override suspend fun getFinalizedBlockHash(chainId: ChainId): BlockHash {
        return rpcCalls.getFinalizedHead(chainId)
    }

    private fun weightedAverageBlockTime(
        sampledBlockTime: SampledBlockTime,
        blockTimeFromConstants: BigInteger
    ): BigInteger {
        val cappedSampleSize = sampledBlockTime.sampleSize.min(REQUIRED_SAMPLED_BLOCKS)
        val sampledPart = cappedSampleSize * sampledBlockTime.averageBlockTime
        val constantsPart = (REQUIRED_SAMPLED_BLOCKS - cappedSampleSize) * blockTimeFromConstants

        return (sampledPart + constantsPart) / REQUIRED_SAMPLED_BLOCKS
    }

    private fun blockTimeFromConstants(chain: Chain, runtime: RuntimeSnapshot): BigInteger {
        return chain.additional?.defaultBlockTimeMillis?.toBigInteger()
            ?: runtime.metadata.babeOrNull()?.numberConstant("ExpectedBlockTime", runtime)
            // Some chains incorrectly use these, i.e. it is set to values such as 0 or even 2
            // Use a low minimum validity threshold to check these against
            ?: runtime.metadata.timestampOrNull()?.numberConstant("MinimumPeriod", runtime)?.takeIf { it > PERIOD_VALIDITY_THRESHOLD.toBigInteger() }
            ?: fallbackBlockTime(runtime)
    }

    private fun fallbackBlockTime(runtime: RuntimeSnapshot): BigInteger {
        return if (runtime.isParachain()) {
            FALLBACK_BLOCK_TIME_MILLIS_PARACHAIN.toBigInteger()
        } else {
            FALLBACK_BLOCK_TIME_MILLIS_RELAYCHAIN.toBigInteger()
        }
    }
}
