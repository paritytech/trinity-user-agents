package io.paritytech.polkadotapp.feature_swap_impl.data.paths

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.common.data.memory.ComputationalCache
import io.paritytech.polkadotapp.common.utils.graph.EdgeVisitFilter
import io.paritytech.polkadotapp.common.utils.graph.Graph
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.findDijkstraPathsBetween
import io.paritytech.polkadotapp.common.utils.graph.numberOfEdges
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.common.utils.measureExecution
import io.paritytech.polkadotapp.feature_swap_api.domain.model.PathRoughFeeEstimation
import io.paritytech.polkadotapp.feature_swap_api.domain.model.QuotedEdge
import io.paritytech.polkadotapp.feature_swap_api.domain.model.QuotedPath
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapDirection
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapGraphEdge
import io.paritytech.polkadotapp.feature_swap_impl.data.paths.model.BestPathQuote
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import javax.inject.Inject

private const val PATHS_LIMIT = 4
private const val QUOTES_CACHE = "RealSwapService.QuotesCache"

class RealPathQuoterFactory @Inject constructor(
    private val computationalCache: ComputationalCache,
) : PathQuoter.Factory {
    override fun <E : SwapGraphEdge> create(
        graphFlow: Flow<Graph<FullChainAssetId, E>>,
        computationalScope: CoroutineScope,
        pathFeeEstimation: PathFeeEstimator<E>?,
        filter: EdgeVisitFilter<E>?
    ): PathQuoter<E> {
        return RealPathQuoter(computationalCache, graphFlow, computationalScope, pathFeeEstimation, filter)
    }
}

private class RealPathQuoter<E : SwapGraphEdge>(
    private val computationalCache: ComputationalCache,
    private val graphFlow: Flow<Graph<FullChainAssetId, E>>,
    private val computationalScope: CoroutineScope,
    private val pathFeeEstimation: PathFeeEstimator<E>?,
    private val filter: EdgeVisitFilter<E>?,
) : PathQuoter<E> {
    override suspend fun findBestPath(
        chainAssetIn: Chain.Asset,
        chainAssetOut: Chain.Asset,
        amount: Balance,
        swapDirection: SwapDirection,
    ): BestPathQuote<E> {
        val from = chainAssetIn.fullId
        val to = chainAssetOut.fullId

        val paths = pathsFromCacheOrCompute(from, to, computationalScope) { graph ->
            val paths = measureExecution("Finding ${chainAssetIn.symbol} -> ${chainAssetOut.symbol} paths") {
                graph.findDijkstraPathsBetween(from, to, limit = PATHS_LIMIT, filter)
            }

            paths
        }

        val quotedPaths = paths.mapAsync { path -> quotePath(path, amount, swapDirection) }
            .filterNotNull()

        if (quotedPaths.isEmpty()) {
            error("Failed to quote: no paths found ${chainAssetIn.symbol} ${(chainAssetIn.chainId)} -> ${chainAssetOut.symbol} (${chainAssetOut.chainId})")
        }

        return BestPathQuote(quotedPaths)
    }

    private suspend fun pathsFromCacheOrCompute(
        from: FullChainAssetId,
        to: FullChainAssetId,
        scope: CoroutineScope,
        computation: suspend (graph: Graph<FullChainAssetId, E>) -> List<Path<E>>
    ): List<Path<E>> {
        val graph = graphFlow.first()

        val cacheKey = "$QUOTES_CACHE:${pathsCacheKey(from, to)}:${graph.numberOfEdges()}"

        return computationalCache.useCache(cacheKey, scope) {
            computation(graph)
        }
    }

    private fun pathsCacheKey(from: FullChainAssetId, to: FullChainAssetId): String {
        val fromKey = "${from.chainId}:${from.assetId}"
        val toKey = "${to.chainId}:${to.assetId}"

        return "$fromKey:$toKey"
    }

    private suspend fun quotePath(
        path: Path<E>,
        amount: Balance,
        swapDirection: SwapDirection
    ): QuotedPath<E>? {
        val quote = when (swapDirection) {
            SwapDirection.SPECIFIED_IN -> quotePathSell(path, amount)
            SwapDirection.SPECIFIED_OUT -> quotePathBuy(path, amount)
        } ?: return null

        val pathRoughFeeEstimation = pathFeeEstimation.roughlyEstimateFeeOrZero(quote)

        return QuotedPath(swapDirection, quote, pathRoughFeeEstimation)
    }

    private suspend fun PathFeeEstimator<E>?.roughlyEstimateFeeOrZero(quote: Path<QuotedEdge<E>>): PathRoughFeeEstimation {
        return this?.roughlyEstimateFee(quote) ?: PathRoughFeeEstimation.zero()
    }

    private suspend fun quotePathBuy(path: Path<E>, amount: Balance): Path<QuotedEdge<E>>? {
        return runCatching {
            val initial = mutableListOf<QuotedEdge<E>>() to amount

            path.foldRight(initial) { segment, (quotedPath, currentAmount) ->
                val segmentQuote = segment.quote(currentAmount, SwapDirection.SPECIFIED_OUT)
                quotedPath.add(0, QuotedEdge(currentAmount, segmentQuote, segment))

                quotedPath to segmentQuote
            }.first
        }
            .logFailure("Failed to quote path")
            .getOrNull()
    }

    private suspend fun quotePathSell(path: Path<E>, amount: Balance): Path<QuotedEdge<E>>? {
        return runCatching {
            val initial = mutableListOf<QuotedEdge<E>>() to amount

            path.fold(initial) { (quotedPath, currentAmount), segment ->
                val segmentQuote = segment.quote(currentAmount, SwapDirection.SPECIFIED_IN)
                quotedPath.add(QuotedEdge(currentAmount, segmentQuote, segment))

                quotedPath to segmentQuote
            }.first
        }
            .logFailure("Failed to quote path")
            .getOrNull()
    }
}
