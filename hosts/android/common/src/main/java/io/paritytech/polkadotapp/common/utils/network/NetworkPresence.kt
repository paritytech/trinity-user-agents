package io.paritytech.polkadotapp.common.utils.network

import android.net.Network
import kotlinx.coroutines.flow.MutableStateFlow
import java.util.concurrent.ConcurrentHashMap

internal class NetworkPresence {
    private val networks = ConcurrentHashMap.newKeySet<Network>()
    val isAvailable = MutableStateFlow(false)

    fun add(network: Network) = publish { networks.add(network) }

    fun remove(network: Network) = publish { networks.remove(network) }

    private inline fun publish(update: () -> Unit) {
        update()
        isAvailable.value = networks.isNotEmpty()
    }
}
