
package io.paritytech.polkadotapp.chains.network.updaters.system

import io.paritytech.polkadotapp.chains.network.updaters.Updater
import kotlinx.coroutines.flow.Flow

interface UpdateSystem {
    fun start(): Flow<Updater.SideEffect>
}
