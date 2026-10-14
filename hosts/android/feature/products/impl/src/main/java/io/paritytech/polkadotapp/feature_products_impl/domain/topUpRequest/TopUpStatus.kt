package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.chains.network.binding.Balance

/** How far a top-up has got, as RFC-0006 reports it to the product. */
sealed interface TopUpStatus {
    /** Waiting for the amount to appear on the source. */
    data object Detecting : TopUpStatus

    /** Claim in progress. */
    data object Claiming : TopUpStatus

    /**
     * At least the requested amount is the user's, at the best head or the finalized one.
     *
     * Terminal when [finalized]; until then a fork can take it back and this returns to [Claiming].
     */
    data class Claimed(val finalized: Boolean) : TopUpStatus

    /** Terminal: only [actualClaimed], less than was asked for, ever arrived. */
    data class ClaimedPartially(val actualClaimed: Balance) : TopUpStatus

    /** Terminal: nothing arrived. */
    data object NotClaimed : TopUpStatus
}

/**
 * Nothing further will be attempted and nothing can move this any more.
 *
 * [TopUpStatus.Claimed] before finality is deliberately excluded: a fork can still take it back, and a
 * top-up recorded as finished on an inclusion that is later retracted is one nothing would ever revisit.
 */
val TopUpStatus.isTerminal: Boolean
    get() = when (this) {
        is TopUpStatus.Claimed -> finalized
        is TopUpStatus.ClaimedPartially, is TopUpStatus.NotClaimed -> true
        is TopUpStatus.Detecting, is TopUpStatus.Claiming -> false
    }
