package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge

/**
 * Modular group of related host call handlers.
 *
 * Each group registers its handlers onto a [ContainerBridge].
 * Environments select which groups to include, enabling per-environment capabilities.
 */
interface HostCallHandlerGroup {
    fun registerOn(bridge: ContainerBridge)
}
