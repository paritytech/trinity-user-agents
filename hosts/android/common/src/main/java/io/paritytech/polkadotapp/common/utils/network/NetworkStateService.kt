package io.paritytech.polkadotapp.common.utils.network

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.awaitTrue
import kotlinx.coroutines.flow.StateFlow
import timber.log.Timber
import javax.inject.Inject

interface NetworkStateService {
    val isNetworkAvailable: StateFlow<Boolean>
}

suspend fun NetworkStateService.awaitNetworkAvailable() = isNetworkAvailable.awaitTrue()

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
    }
}
