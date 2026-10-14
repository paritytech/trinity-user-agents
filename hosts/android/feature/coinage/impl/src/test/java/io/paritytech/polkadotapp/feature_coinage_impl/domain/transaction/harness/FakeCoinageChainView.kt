package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReader
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReaderFactory
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.RecyclerAliasKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainViewFactory
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TransactionSearchResult
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.searchRange
import io.paritytech.polkadotapp.test_shared.chain.FakeBlock
import io.paritytech.polkadotapp.test_shared.chain.FakeChain
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import java.math.BigInteger

/**
 * Drives a [FakeChain] and hands out views over it, standing in for the chain half of the subsystem.
 *
 * Every read honours the batched-read contract: a successful result carries every requested key, with a
 * null value where the chain holds nothing.
 *
 * Head emissions are ticks: they carry a number, and every read goes back through a freshly pinned view.
 * Producing a block or finalizing announces both heads, which is what makes the recovery loop run.
 */
class FakeCoinageChainViewFactory(
    val chain: FakeChain<CoinageChainState>,
) : PinnedChainViewFactory, CoinageStateReaderFactory {
    var faults: ChainReadFaults = ChainReadFaults.NONE

    private val finalizedHeads = MutableSharedFlow<BlockNumber>(
        replay = 1,
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    private val bestHeads = MutableSharedFlow<BlockNumber>(
        replay = 1,
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    /** How many times a view was pinned, so a scenario can assert a pass did not read the chain at all. */
    var pins = 0
        private set

    override suspend fun pin(chainId: String): Result<PinnedChainView> {
        pins++

        return if (faults.pinFails) {
            Result.failure(ChainReadFailure("pin failed"))
        } else {
            Result.success(FakeCoinageChainView(chain, faults, chain.finalizedHead, chain.bestHead))
        }
    }

    /**
     * The reader reads the same chain the view was pinned over, so a scenario's fault injection covers both
     * halves without having to know they are separate objects.
     */
    override suspend fun create(view: PinnedChainView): CoinageStateReader =
        FakeCoinageChainView(chain, faults, chain.finalizedHead, chain.bestHead)

    override fun finalizedHeads(chainId: String): Flow<BlockNumber> = finalizedHeads

    override fun bestHeads(chainId: String): Flow<BlockNumber> = bestHeads

    fun produceBlock(body: List<TransactionHash> = emptyList(), mutate: (CoinageChainState) -> CoinageChainState = { it }) =
        chain.produceBlock(body, mutate).also { announce() }

    fun finalize(upTo: Long) {
        chain.finalize(upTo)
        announce()
    }

    fun reorg(depth: Int) {
        chain.reorg(depth)
        announce()
    }

    fun announce() {
        bestHeads.tryEmit(BlockNumber(chain.bestHead.number.toBigInteger()))
        finalizedHeads.tryEmit(BlockNumber(chain.finalizedHead.number.toBigInteger()))
    }
}

private class FakeCoinageChainView(
    private val chain: FakeChain<CoinageChainState>,
    private val faults: ChainReadFaults,
    finalized: FakeBlock<CoinageChainState>,
    best: FakeBlock<CoinageChainState>,
) : PinnedChainView, CoinageStateReader {
    override val finalizedHead: CheckpointBlock = finalized.checkpoint()

    override val bestHead: CheckpointBlock = best.checkpoint()

    override suspend fun coinsAt(at: BlockHash, coins: List<AccountId>): Result<Map<AccountId, OnChainCoinInfo?>> {
        val unreadable = coins.filter { it in faults.unreadableCoins }
        if (unreadable.isNotEmpty() || at in faults.statelessBlocks) {
            return Result.failure(ChainReadFailure("coins unreadable at $at"))
        }

        val state = chain.stateAt(at) ?: return Result.failure(ChainReadFailure("no state at $at"))

        return Result.success(coins.associateWith { state.coins[it] })
    }

    override suspend fun recyclerMembershipsAt(
        at: BlockHash,
        memberKeys: List<BandersnatchPublicKey>,
    ): Result<Map<BandersnatchPublicKey, ValueExponent?>> {
        if (faults.membershipsUnreadable) return Result.failure(ChainReadFailure("memberships unreadable at $at"))

        val state = chain.stateAt(at) ?: return Result.failure(ChainReadFailure("no state at $at"))

        return Result.success(memberKeys.associateWith { state.recyclerMembers[it] })
    }

    override suspend fun ringPositionsAt(
        at: BlockHash,
        memberships: Map<BandersnatchPublicKey, ValueExponent>,
    ): Result<Map<BandersnatchPublicKey, RingPosition?>> {
        if (faults.ringPositionsUnreadable) return Result.failure(ChainReadFailure("ring positions unreadable at $at"))

        val state = chain.stateAt(at) ?: return Result.failure(ChainReadFailure("no state at $at"))

        return Result.success(memberships.keys.associateWith { state.ringPositions[it] })
    }

    override suspend fun aliasStatesAt(
        at: BlockHash,
        keys: List<RecyclerAliasKey>,
    ): Result<Map<RecyclerAliasKey, OnChainAliasState?>> {
        val unreadable = keys.filter { it in faults.unreadableAliases }
        if (unreadable.isNotEmpty()) return Result.failure(ChainReadFailure("aliases unreadable at $at"))

        val state = chain.stateAt(at) ?: return Result.failure(ChainReadFailure("no state at $at"))

        return Result.success(keys.associateWith { state.aliases[it] })
    }

    override suspend fun blockHashAt(blockNumber: Long): Result<BlockHash?> =
        if (faults.everyBlockUnreadable || blockNumber in faults.unreadableBlocks) {
            Result.failure(ChainReadFailure("block $blockNumber unreadable"))
        } else {
            Result.success(chain.canonicalAt(blockNumber)?.hash)
        }

    override suspend fun blockNumberAt(hash: BlockHash): Result<Long?> {
        val block = chain.blockAt(hash) ?: return Result.success(null)

        return if (faults.everyBlockUnreadable || block.number in faults.unreadableBlocks) {
            Result.failure(ChainReadFailure("block ${block.number} unreadable"))
        } else {
            Result.success(block.number)
        }
    }

    override suspend fun dispatchOutcomeAt(at: BlockHash, txHash: TransactionHash): Result<ExtrinsicOutcome?> {
        if (txHash in faults.unreadableOutcomes) return Result.failure(ChainReadFailure("outcome of $txHash unreadable"))

        val block = chain.blockAt(at) ?: return Result.failure(ChainReadFailure("no block at $at"))
        if (txHash !in block.body) return Result.success(null)

        return Result.success(block.state.outcomes[txHash])
    }

    override suspend fun getAppliedExtrinsicHashes(at: BlockHash): Result<List<TransactionHash>> {
        val block = chain.blockAt(at) ?: return Result.failure(ChainReadFailure("no block at $at"))

        return if (faults.everyBlockUnreadable || block.number in faults.unreadableBlocks) {
            Result.failure(ChainReadFailure("body of ${block.number} unreadable"))
        } else {
            Result.success(block.body)
        }
    }

    override suspend fun searchForTransaction(
        fromBlockNumber: Long,
        toBlockNumber: Long,
        txHash: TransactionHash,
    ): TransactionSearchResult =
        if (faults.txSearchDisabled) {
            TransactionSearchResult.NotFound(wholeRangeRead = false)
        } else {
            searchRange(fromBlockNumber, toBlockNumber, txHash)
        }
}

private fun FakeBlock<CoinageChainState>.checkpoint() = CheckpointBlock(blockNumber = number, blockHash = hash)

fun recyclerAliasKey(valueExponent: Int, recyclerIndex: Int, alias: DataByteArray) =
    RecyclerAliasKey(
        valueExponent = BigInteger.valueOf(valueExponent.toLong()),
        recyclerIndex = BigInteger.valueOf(recyclerIndex.toLong()),
        alias = alias,
    )
