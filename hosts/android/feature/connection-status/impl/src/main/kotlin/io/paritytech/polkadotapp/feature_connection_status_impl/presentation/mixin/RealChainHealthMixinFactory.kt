package io.paritytech.polkadotapp.feature_connection_status_impl.presentation.mixin

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_connection_status_api.domain.ChainHealthMonitor
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthMixin
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStorePeer
import javax.inject.Inject

class RealChainHealthMixinFactory @Inject constructor(
    private val monitor: ChainHealthMonitor,
    private val peer: StatementStorePeer,
    private val knownChains: KnownChains,
    private val appLifecycleObserver: AppLifecycleObserver,
) : ChainHealthMixin.Factory {
    override fun create(scope: ComputationalScope): ChainHealthMixin =
        RealChainHealthMixin(scope, monitor, peer, knownChains, appLifecycleObserver)
}
