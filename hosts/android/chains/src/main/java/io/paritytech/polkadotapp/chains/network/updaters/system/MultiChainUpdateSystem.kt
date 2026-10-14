package io.paritytech.polkadotapp.chains.network.updaters.system

import io.paritytech.polkadotapp.chains.network.updaters.Updater
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.merge

class MultiChainUpdateSystem(
    private val nested: List<UpdateSystem>
) : UpdateSystem {
    constructor(vararg nested: UpdateSystem) : this(nested.toList())

    override fun start(): Flow<Updater.SideEffect> {
        return nested
            .map { it.start() }
            .merge()
    }
}
