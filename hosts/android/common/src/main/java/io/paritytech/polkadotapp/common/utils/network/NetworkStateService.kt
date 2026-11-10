package io.paritytech.polkadotapp.common.utils.network

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.awaitTrue
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filterNotNull
import timber.log.Timber
import javax.inject.Inject

interface NetworkStateService {
    val isNetworkAvailable: StateFlow<Boolean>

    /** The network the system sends app traffic over, or null while there is none. */
    val defaultNetwork: StateFlow<Network?>
}

suspend fun NetworkStateService.awaitNetworkAvailable() = isNetworkAvailable.awaitTrue()

/** Each network `defaultNetwork` moves to, never the one already in use when collection starts. */
fun Flow<Network?>.networkSwitches(): Flow<Network> = filterNotNull().distinctUntilChanged().drop(1)

suspend fun <T> NetworkStateService.withNetworkRetries(compute: suspend () -> T) =
    runCatching { compute() }
        .recoverCatching {
            if (isNetworkAvailable.value) throw it

            Timber.d("Failed to perform compute due to internet connectivity. Waiting for network to be available...")

            awaitNetworkAvailable()

            Timber.d("Network is available. Retrying...")

            compute()
        }

@SuppressLint("MissingPermission")
class RealNetworkStateService @Inject constructor(@ApplicationContext context: Context) : NetworkStateService {
    private val presence = NetworkPresence()
    override val isNetworkAvailable: StateFlow<Boolean> = presence.isAvailable

    private val currentDefaultNetwork = MutableStateFlow<Network?>(null)
    override val defaultNetwork: StateFlow<Network?> = currentDefaultNetwork.asStateFlow()

    init {
        val connectivityManager = context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val networkRequest = NetworkRequest.Builder()
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()

        connectivityManager.registerNetworkCallback(
            networkRequest,
            object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) = presence.add(network)

                override fun onLost(network: Network) = presence.remove(network)
            })

        connectivityManager.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                currentDefaultNetwork.value = network
            }

            // A late loss of the previous default must not clear the one that replaced it.
            override fun onLost(network: Network) {
                currentDefaultNetwork.compareAndSet(network, null)
            }
        })
    }
}
