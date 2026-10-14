package io.paritytech.polkadotapp.feature_connection_status_impl.presentation.mixin.mapper

import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainHealth
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainMetricReading
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator

internal fun ChainHealth.toIndicator(): ChainHealthIndicator = when (connection) {
    ChainConnectionPresentation.Offline -> ChainHealthIndicator.Offline
    ChainConnectionPresentation.Disconnected -> ChainHealthIndicator.Disconnected
    ChainConnectionPresentation.Connecting -> ChainHealthIndicator.Connecting
    ChainConnectionPresentation.Connected -> when {
        nodeSilent() -> ChainHealthIndicator.Disconnected
        else -> ChainHealthIndicator.of(productionShare(), expectedBlockTime)
    }
}

internal fun ChainHealth.isAnchorPending(): Boolean = readings
    .filterIsInstance<ChainMetricReading.BlockProduction>()
    .any { it.anchorPending }

private fun ChainHealth.productionShare(): Float? = readings
    .filterIsInstance<ChainMetricReading.BlockProduction>()
    .firstOrNull()
    ?.share

private fun ChainHealth.nodeSilent(): Boolean = readings
    .filterIsInstance<ChainMetricReading.UnansweredRequest>()
    .any { it.isOverLimit }
