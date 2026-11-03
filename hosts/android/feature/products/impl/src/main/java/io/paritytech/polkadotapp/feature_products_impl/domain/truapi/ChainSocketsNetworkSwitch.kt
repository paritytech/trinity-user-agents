package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.WebSocketChainProvider
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.common.utils.network.networkSwitches
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import timber.log.Timber

/**
 * Closes every chain socket of [chainProvider] each time the system moves app traffic to another
 * network.
 *
 * A socket stays on the network it was opened on, and one left on a network the phone moved away
 * from can sit dead until a ping goes unanswered. closeAll reports each connection back to the
 * core, which dials again on the new network on next use.
 */
internal fun CoroutineScope.closeChainSocketsOnNetworkSwitch(
    networkStateService: NetworkStateService,
    chainProvider: WebSocketChainProvider,
): Job = launch {
    networkStateService.defaultNetwork.networkSwitches().collect {
        Timber.tag("truapi.chain").i("default network changed, closing the chain sockets")
        chainProvider.closeAll()
    }
}
