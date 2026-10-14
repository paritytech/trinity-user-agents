package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

class QuotedEdge<E>(
    val quotedAmount: Balance,
    val quote: Balance,
    val edge: E
)
