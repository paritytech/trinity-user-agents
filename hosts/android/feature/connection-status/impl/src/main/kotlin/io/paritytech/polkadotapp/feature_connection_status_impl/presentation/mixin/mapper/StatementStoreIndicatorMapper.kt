package io.paritytech.polkadotapp.feature_connection_status_impl.presentation.mixin.mapper

import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainHealth
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator

internal fun ChainHealth?.statementStoreIndicator(answered: Boolean): ChainHealthIndicator =
    when (this?.connection) {
        null, ChainConnectionPresentation.Connecting -> ChainHealthIndicator.Connecting
        ChainConnectionPresentation.Disconnected -> ChainHealthIndicator.Disconnected
        ChainConnectionPresentation.Offline -> ChainHealthIndicator.Offline
        // A node can hold the socket and still not serve statements; only an answered subscription rules that out.
        ChainConnectionPresentation.Connected ->
            if (answered) ChainHealthIndicator.Healthy(liveness = null) else ChainHealthIndicator.Connecting
    }
