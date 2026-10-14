package io.paritytech.polkadotapp.common.presentation

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Runs every [AppInitializer] contributed by feature modules. Failures from an individual
 * initializer are logged and swallowed so one bad actor cannot prevent the rest from running.
 */
@Singleton
class AppInitializerPipeline @Inject constructor(
    private val initializers: Set<@JvmSuppressWildcards AppInitializer>
) {
    context(scope: ComputationalScope)
    fun initialize() {
        initializers.forEach { initializer ->
            initializer.initialize()
                .onFailure { Timber.e(it, "App initializer ${initializer::class.simpleName} failed") }
        }
    }
}
