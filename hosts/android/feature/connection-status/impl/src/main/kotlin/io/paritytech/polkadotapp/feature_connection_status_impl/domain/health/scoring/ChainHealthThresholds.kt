package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.scoring

import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

object ChainHealthThresholds {
    // Re-evaluation cadence, so a share decays during a stall instead of holding its last value.
    val SAMPLE_TICK: Duration = 1.seconds

    // Ten slots at least: with five, each colour band is one block wide and one late block skips two bands.
    val BLOCK_PRODUCTION_MIN_WINDOW: Duration = 30.seconds
    const val BLOCK_PRODUCTION_MIN_SLOTS = 10

    val BLOCK_PRODUCTION_ANCHOR_TIMEOUT: Duration = 15.seconds

    const val UNANSWERED_REQUEST_BLOCK_TIMES = 3
}
