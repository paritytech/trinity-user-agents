package io.paritytech.polkadotapp.chains.network.updaters.scope

import io.paritytech.polkadotapp.chains.network.updaters.Updater
import kotlinx.coroutines.flow.flowOf

object GlobalUpdaterScope : Updater.NoChainScope<Unit> {
    override fun invalidationFlow() = flowOf(Unit)
}

interface GlobalScopeUpdater : Updater<Unit> {
    override val scope: Updater.Scope<Unit>
        get() = GlobalUpdaterScope
}
