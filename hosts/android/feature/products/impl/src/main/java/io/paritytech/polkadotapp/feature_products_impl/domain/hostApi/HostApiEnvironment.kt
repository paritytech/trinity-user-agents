package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups.HostCallHandlerGroup

/**
 * Composed configuration for a host API environment.
 *
 * Each environment (chat, SPA browser, explore) assembles its own [HostApiEnvironment]
 * by composing the appropriate strategies. [HostApiSession] uses this to initialize.
 */
class HostApiEnvironment(
    val injectionStrategy: ContainerInjectionStrategy,
    val handlerGroups: List<HostCallHandlerGroup>,
)
