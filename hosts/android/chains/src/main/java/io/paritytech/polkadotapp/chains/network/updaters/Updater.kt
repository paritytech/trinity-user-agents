package io.paritytech.polkadotapp.chains.network.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.transform

interface Updater<S> {
    val scope: Scope<S>

    /**
     * Implementations should be aware of cancellation
     */
    suspend fun listenForUpdates(
        scopeValue: S,
        context: Context
    ): Flow<SideEffect>

    interface SideEffect

    class Context(
        val storageSubscriptionBuilder: SharedRequestsBuilder,
        val chain: Chain
    )

    interface Scope<S> {
        fun invalidationFlow(chain: Chain): Flow<S>
    }

    interface NoChainScope<S> : Scope<S> {
        fun invalidationFlow(): Flow<S>

        override fun invalidationFlow(chain: Chain) = invalidationFlow()
    }
}

fun <T> Flow<T>.noSideAffects(): Flow<Updater.SideEffect> = transform { }
