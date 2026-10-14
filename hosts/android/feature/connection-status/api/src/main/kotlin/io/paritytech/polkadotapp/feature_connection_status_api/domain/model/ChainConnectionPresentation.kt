package io.paritytech.polkadotapp.feature_connection_status_api.domain.model

/**
 * Smoothed, user-facing connection state for a chain. Distinct from the raw socket state:
 * transient reconnect flapping is collapsed into [Connecting] rather than oscillating.
 */
sealed interface ChainConnectionPresentation {
    data object Connected : ChainConnectionPresentation
    data object Connecting : ChainConnectionPresentation
    data object Disconnected : ChainConnectionPresentation

    /** The device itself has no network; nothing is known about the chain. */
    data object Offline : ChainConnectionPresentation
}
