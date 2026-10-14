package io.paritytech.polkadotapp.feature_transactions.api.domain.durable

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import kotlinx.coroutines.flow.Flow

/** Where a transaction was found, and whether its dispatch succeeded there. */
sealed interface TransactionSearchResult {
    /** [outcome] is null when the events at [block] could not be read — inclusion is not success. */
    data class Found(val block: CheckpointBlock, val outcome: ExtrinsicOutcome?) : TransactionSearchResult

    /** [wholeRangeRead] is false when some block could not be read, so absence proves nothing. */
    data class NotFound(val wholeRangeRead: Boolean) : TransactionSearchResult
}

/**
 * One view of the chain, pinned for the length of a single recovery pass.
 *
 * These are the reads that are true of any transaction: where a block is, what is in its body, and whether
 * a dispatch succeeded. Anything domain-shaped is read by the domain's own [TxCompletionOracle], at the
 * hashes this view pins.
 */
interface PinnedChainView {
    val finalizedHead: CheckpointBlock

    val bestHead: CheckpointBlock

    /** The canonical hash at [blockNumber], for checking that a block we recorded is still real. */
    suspend fun blockHashAt(blockNumber: Long): Result<BlockHash?>

    suspend fun blockNumberAt(hash: BlockHash): Result<Long?>

    /**
     * Whether [txHash] dispatched successfully at [at]. The index is resolved from the same block the events
     * are read from and never stored — a reorg reorders extrinsics, so a stored index would name someone
     * else's outcome. Null when the transaction is not in that block.
     */
    suspend fun dispatchOutcomeAt(at: BlockHash, txHash: TransactionHash): Result<ExtrinsicOutcome?>

    suspend fun getAppliedExtrinsicHashes(at: BlockHash): Result<List<TransactionHash>>

    /**
     * Looks for [txHash] in `[from … to]`, and reads its dispatch outcome where it is found.
     *
     * Callers bound [toBlockNumber] at the finalized head, so a hit is always finalized and a terminal
     * verdict from it rests on a finalized fact.
     */
    suspend fun searchForTransaction(
        fromBlockNumber: Long,
        toBlockNumber: Long,
        txHash: TransactionHash,
    ): TransactionSearchResult
}

/** Pins one view per chain per pass. */
interface PinnedChainViewFactory {
    suspend fun pin(chainId: ChainId): Result<PinnedChainView>

    /**
     * Emits every newly finalized block, so recovery can run a pass exactly when the facts it reads can have
     * changed. The number is a tick and nothing more — each pass still pins its own view.
     */
    fun finalizedHeads(chainId: ChainId): Flow<BlockNumber>

    /**
     * Emits every new best block. Pre-finality success is read at the best head, so those facts move here
     * rather than at finality — several blocks earlier.
     */
    fun bestHeads(chainId: ChainId): Flow<BlockNumber>
}
