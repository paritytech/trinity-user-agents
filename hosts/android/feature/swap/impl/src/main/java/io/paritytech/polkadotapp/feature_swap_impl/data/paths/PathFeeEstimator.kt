package io.paritytech.polkadotapp.feature_swap_impl.data.paths

import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.feature_swap_api.domain.model.PathRoughFeeEstimation
import io.paritytech.polkadotapp.feature_swap_api.domain.model.QuotedEdge

interface PathFeeEstimator<E> {
    suspend fun roughlyEstimateFee(path: Path<QuotedEdge<E>>): PathRoughFeeEstimation
}
