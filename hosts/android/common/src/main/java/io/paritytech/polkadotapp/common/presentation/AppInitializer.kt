package io.paritytech.polkadotapp.common.presentation

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope

/**
 * A unit of startup work that a feature contributes to the app.
 *
 * Implementations are registered via multibindings (`@Binds @IntoSet`) and executed once by
 * [AppInitializerPipeline]. The caller provides a [ComputationalScope] so long-lived work
 * (e.g. flow subscriptions) is tied to the caller's lifecycle.
 *
 * Returning [Result] prevents a failing initializer from crashing the app; the pipeline logs
 * the failure and continues with the rest.
 */
interface AppInitializer {
    context(scope: ComputationalScope)
    fun initialize(): Result<Unit>
}
