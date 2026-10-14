package io.paritytech.polkadotapp.feature_connection_status_api.domain.model

import kotlin.time.Duration

/**
 * One measurement's contribution to a chain's health. Adding a metric adds a variant here, a probe that
 * emits it, and a mapper branch that folds it into the indicator.
 */
sealed interface ChainMetricReading {
    /**
     * Blocks the chain produced across the trailing window, as the height difference across it, against the
     * [expectedBlocks] its configured block time predicts. [producedBlocks] is null while no sample reaches
     * back a full window: not measured yet, never stalled. [anchorPending] is true while the chain is still
     * being asked how fast it has been going, so a consumer can keep showing what it showed before.
     */
    data class BlockProduction(
        val producedBlocks: Int?,
        val expectedBlocks: Int,
        val anchorPending: Boolean,
    ) : ChainMetricReading {
        val share: Float?
            get() = producedBlocks?.let { (it.toFloat() / expectedBlocks).coerceAtMost(1f) }
    }

    /** How long the oldest still-pending socket request has waited; past [limit] the node counts as silent. */
    data class UnansweredRequest(
        val age: Duration,
        val limit: Duration,
    ) : ChainMetricReading {
        val isOverLimit: Boolean
            get() = age > limit
    }
}
