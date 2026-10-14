package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import kotlin.time.Duration

data class BlockProductionAnchor(
    val headHeight: Int,
    val blocks: Int,
    val chainClockSpan: Duration,
)
