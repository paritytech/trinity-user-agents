package io.paritytech.polkadotapp.feature_swap_impl.data.paths

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.graph.EdgeVisitFilter
import io.paritytech.polkadotapp.common.utils.graph.Graph
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapDirection
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapGraphEdge
import io.paritytech.polkadotapp.feature_swap_impl.data.paths.model.BestPathQuote
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow

interface PathQuoter<E : SwapGraphEdge> {
    interface Factory {
        fun <E : SwapGraphEdge> create(
            graphFlow: Flow<Graph<FullChainAssetId, E>>,
            computationalScope: CoroutineScope,
            pathFeeEstimation: PathFeeEstimator<E>? = null,
            filter: EdgeVisitFilter<E>? = null
        ): PathQuoter<E>
    }

    suspend fun findBestPath(
        chainAssetIn: Chain.Asset,
        chainAssetOut: Chain.Asset,
        amount: Balance,
        swapDirection: SwapDirection,
    ): BestPathQuote<E>
}
