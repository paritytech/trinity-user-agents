package io.paritytech.polkadotapp.feature_transactions_impl.data.durable

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ChainEventsRepositoryFactory
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.getExtrinsicWithEvents
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.status
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.network.rpc.getBlockNumber
import io.paritytech.polkadotapp.chains.util.extrinsicHash
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainViewFactory
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TransactionSearchResult
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.durabilityLogD
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.durabilityLogW
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.shortHash
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import java.math.BigInteger
import javax.inject.Inject

class RealPinnedChainViewFactory @Inject constructor(
    private val chainEventsRepositoryFactory: ChainEventsRepositoryFactory,
    private val rpcCalls: RpcCalls,
) : PinnedChainViewFactory {
    override suspend fun pin(chainId: ChainId): Result<PinnedChainView> = runCatching {
        coroutineScope {
            val finalizedHash = rpcCalls.getFinalizedHead(chainId)
            val bestHash = rpcCalls.getBlockHash(chainId)

            val finalizedNumber = async { rpcCalls.getBlockNumber(chainId, finalizedHash).toLong() }
            val bestNumber = async { rpcCalls.getBlockNumber(chainId, bestHash).toLong() }

            RealPinnedChainView(
                chainId = chainId,
                finalizedHead = CheckpointBlock(finalizedNumber.await(), finalizedHash),
                bestHead = CheckpointBlock(bestNumber.await(), bestHash),
                chainEventsRepositoryFactory = chainEventsRepositoryFactory,
                rpcCalls = rpcCalls,
            )
        }
    }.onFailure {
        // A failed pin aborts a whole recovery pass before anything else can be logged about it.
        durabilityLogW("chain-view-pin-failed error=$it")
    }

    override fun finalizedHeads(chainId: ChainId): Flow<BlockNumber> = flowOfAll {
        rpcCalls.subscribeFinalizedHeads(chainId)
    }.map { BlockNumber(it.number.toBigInteger()) }

    override fun bestHeads(chainId: ChainId): Flow<BlockNumber> = flowOfAll {
        rpcCalls.subscribeNewHeads(chainId)
    }.map { BlockNumber(it.number.toBigInteger()) }
}

private class RealPinnedChainView(
    private val chainId: ChainId,
    override val finalizedHead: CheckpointBlock,
    override val bestHead: CheckpointBlock,
    private val chainEventsRepositoryFactory: ChainEventsRepositoryFactory,
    private val rpcCalls: RpcCalls,
) : PinnedChainView {
    override suspend fun blockHashAt(blockNumber: Long): Result<BlockHash?> = runCatching {
        rpcCalls.getBlockHash(chainId, BlockNumber(BigInteger.valueOf(blockNumber)))
    }

    override suspend fun blockNumberAt(hash: BlockHash): Result<Long?> = runCatching {
        rpcCalls.getBlockNumber(chainId, hash).toLong()
    }

    override suspend fun dispatchOutcomeAt(at: BlockHash, txHash: TransactionHash): Result<ExtrinsicOutcome?> =
        runCatching {
            chainEventsRepositoryFactory.create(chainId)
                .getExtrinsicWithEvents(txHash, at)
                ?.status()
        }

    override suspend fun getAppliedExtrinsicHashes(at: BlockHash): Result<List<TransactionHash>> = runCatching {
        rpcCalls.getBlock(chainId, at).block.extrinsics.map { it.extrinsicHash() }
    }

    override suspend fun searchForTransaction(
        fromBlockNumber: Long,
        toBlockNumber: Long,
        txHash: TransactionHash,
    ): TransactionSearchResult = searchRange(fromBlockNumber, toBlockNumber, txHash)
}

/**
 * Walks `[fromBlockNumber … toBlockNumber]` looking for [txHash], reading the block hashes and bodies
 * concurrently.
 *
 * An unreadable block only clears [TransactionSearchResult.NotFound.wholeRangeRead] — the search keeps
 * going, because a hit is positive evidence regardless of what could not be read on the way, while a miss
 * only means something if every block was actually looked at.
 */
suspend fun PinnedChainView.searchRange(
    fromBlockNumber: Long,
    toBlockNumber: Long,
    txHash: TransactionHash,
): TransactionSearchResult = coroutineScope {
    val blocks = (fromBlockNumber..toBlockNumber).map { number ->
        number to async {
            val hash = blockHashAt(number).getOrNull()
            hash to hash?.let { getAppliedExtrinsicHashes(it).getOrNull() }
        }
    }

    var unreadableBlocks = 0

    for ((number, deferred) in blocks) {
        val (hash, body) = deferred.await()

        if (hash == null || body == null) {
            unreadableBlocks++
            continue
        }

        if (txHash !in body) continue

        // The body is already in hand, so the outcome costs one further read of that block's events.
        val outcome = dispatchOutcomeAt(hash, txHash).getOrNull()

        durabilityLogD("tx-search found tx=${txHash.shortHash()} block=$number outcome=$outcome unreadable=$unreadableBlocks")

        return@coroutineScope TransactionSearchResult.Found(CheckpointBlock(number, hash), outcome)
    }

    if (unreadableBlocks > 0) {
        durabilityLogW(
            "tx-search absence-inconclusive tx=${txHash.shortHash()} " +
                "range=$fromBlockNumber..$toBlockNumber unreadable=$unreadableBlocks"
        )
    } else {
        durabilityLogD("tx-search absent tx=${txHash.shortHash()} range=$fromBlockNumber..$toBlockNumber")
    }

    TransactionSearchResult.NotFound(wholeRangeRead = unreadableBlocks == 0)
}
