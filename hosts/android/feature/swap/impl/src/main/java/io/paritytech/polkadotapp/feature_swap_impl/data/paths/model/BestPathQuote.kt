package io.paritytech.polkadotapp.feature_swap_impl.data.paths.model

import io.paritytech.polkadotapp.feature_swap_api.domain.model.QuotedPath

class BestPathQuote<E>(
    val candidates: List<QuotedPath<E>>
) {
    val bestPath: QuotedPath<E> = candidates.max()
}
