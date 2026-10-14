package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.graph.Graph
import io.paritytech.polkadotapp.common.utils.graph.WeightedEdge

interface SwapGraphEdge : WeightedEdge<FullChainAssetId> {
    suspend fun quote(
        amount: Balance,
        direction: SwapDirection
    ): Balance

    /**
     * Begin a fully-constructed, ready to submit operation
     */
    suspend fun beginOperation(args: AtomicSwapOperationArgs): AtomicSwapOperation

    /**
     * Append current swap edge execution to the existing transaction
     * Return null if it is not possible, indicating that the new transaction should be initiated to handle this edge via
     * [beginOperation]
     */
    suspend fun appendToOperation(currentTransaction: AtomicSwapOperation, args: AtomicSwapOperationArgs): AtomicSwapOperation?

    /**
     * Begin a operation prototype that should reflect similar structure to [beginOperation] and [appendToOperation] but is limited to available functionality
     * Used during quoting to construct the operations array when not all parameters are still known
     */
    suspend fun beginOperationPrototype(): AtomicSwapOperationPrototype

    /**
     * Append current swap edge execution to the existing transaction prototype
     * Return null if it is not possible, indicating that the new transaction should be initiated to handle this edge via
     * [beginOperationPrototype]
     */
    suspend fun appendToOperationPrototype(currentTransaction: AtomicSwapOperationPrototype): AtomicSwapOperationPrototype?

    /**
     * Debug label to describe this edge for logging
     */
    suspend fun debugLabel(): String?

    /**
     * Whether this Edge fee check should be skipped when adding to after a specified [predecessor]
     * The main purpose is to mirror the behavior of [appendToOperation] - multiple segments appended together
     * most likely will only use fee configuration from the first segment in the batch
     * Note that returning true here means that [canPayNonNativeFeesInIntermediatePosition] wont be called and checked
     */
    fun predecessorHandlesFees(predecessor: SwapGraphEdge): Boolean

    /**
     * Can be used to define additional restrictions on top of default one, "is able to pay submission fee on origin"
     * This will only be called for intermediate hops for non-utility assets since other cases are always payable
     */
    suspend fun canPayNonNativeFeesInIntermediatePosition(): Boolean

    /**
     * Determines whether it is possible to spend whole asset-in to receive asset-out
     * This has to be true for edge to be considered a valid intermediate segment
     * since we don't want dust leftovers across intermediate segments
     */
    suspend fun canTransferOutWholeAccountBalance(): Boolean
}

typealias SwapGraph = Graph<FullChainAssetId, SwapGraphEdge>
