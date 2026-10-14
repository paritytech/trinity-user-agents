package io.paritytech.polkadotapp.feature_transactions_impl.data

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.Era
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.toBlockNumber
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.invoke
import io.paritytech.polkadotapp.feature_transactions.api.data.Mortality
import kotlinx.coroutines.async
import kotlinx.coroutines.withContext
import java.lang.Integer.min
import javax.inject.Inject
import javax.inject.Singleton

private const val FALLBACK_MAX_HASH_COUNT = 250
private const val FINALITY_BUFFER = 5
private const val MORTAL_PERIOD = 2 * 60 * 1000

@Singleton
class MortalityConstructor @Inject constructor(
    private val chainStateRepository: ChainStateRepository,
    private val coroutineDispatchers: CoroutineDispatchers
) {
    fun mortalPeriodMillis(): Long = MORTAL_PERIOD.toLong()

    suspend fun constructMortality(
        chainId: ChainId,
        rpcCalls: RpcCalls
    ): Mortality = withContext(coroutineDispatchers.io) {
        val finalizedHash = async { rpcCalls.getFinalizedHead(chainId) }

        val bestHeader = async { rpcCalls.getBlockHeader(chainId) }
        val finalizedHeader = async { rpcCalls.getBlockHeader(chainId, finalizedHash()) }

        val currentHeader = async { bestHeader().parentHash?.let { rpcCalls.getBlockHeader(chainId, it) } ?: bestHeader() }

        val currentNumber = currentHeader().number
        val finalizedNumber = finalizedHeader().number

        // Always anchor against the finalized block. Anchoring to a non-finalized (best) block is reorg-prone:
        // a reorg changes the era's birth-block hash, which is part of the signed payload, so the runtime
        // rejects the tx as BadProof. Finalized blocks never reorg, so the anchor stays valid.
        val startBlockNumber = finalizedNumber

        val blockHashCount = chainStateRepository.blockHashCount(chainId)?.toInt()

        val blockTime = chainStateRepository.expectedBlockTime(chainId).inWholeMilliseconds.toInt()

        // Extend the era to cover the finalized->best gap, so a tx anchored at the (older) finalized block still
        // lives the intended duration from the current best block. Still capped by blockHashCount below.
        val finalityLag = (currentNumber - finalizedNumber).coerceAtLeast(0)
        val mortalPeriod = MORTAL_PERIOD / blockTime + finalityLag + FINALITY_BUFFER

        val unmappedPeriod = min(blockHashCount ?: FALLBACK_MAX_HASH_COUNT, mortalPeriod)

        val era = Era.getEraFromBlockPeriod(startBlockNumber, unmappedPeriod)
        val eraBlockNumber = ((startBlockNumber - era.phase) / era.period) * era.period + era.phase

        val eraBlockHash = rpcCalls.getBlockHash(chainId, eraBlockNumber.toBlockNumber())
            .fromHex().toDataByteArray()

        Mortality(era, eraBlockHash, eraBlockNumber.toLong())
    }
}
