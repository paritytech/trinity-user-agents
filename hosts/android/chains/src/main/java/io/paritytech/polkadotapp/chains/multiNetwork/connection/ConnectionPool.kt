package io.paritytech.polkadotapp.chains.multiNetwork.connection

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ConnectionPool @Inject constructor(
    private val chainConnectionFactory: ChainConnectionFactory,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val appLifecycleObserver: AppLifecycleObserver
) {
    private val scope = CoroutineScope(Dispatchers.Default)
    private val pool = ConcurrentHashMap<String, ChainConnection>()
    private val lifecycleRefs = ConcurrentHashMap<ChainId, EnabledChainConnectionReference>()

    init {
        appLifecycleObserver.subscribe().onEach { state ->
            when (state) {
                AppLifecycleState.FOREGROUND -> acquireAllLifecycleRefs()
                AppLifecycleState.BACKGROUND -> releaseAllLifecycleRefs()
            }
        }.launchIn(scope)
    }

    fun getConnection(chainId: String): ChainConnection = pool.getValue(chainId)

    fun getConnectionOrNull(chainId: String): ChainConnection? = pool[chainId]

    suspend fun setupConnection(chain: Chain): ChainConnection {
        val connection =
            pool.getOrPut(chain.id) {
                chainConnectionFactory.create(chain)
                    .also { acquireLifecycleRefIfForeground(chain) }
            }

        connection.considerUpdateNodes(chain.nodes)

        return connection
    }

    fun removeConnection(chainId: String) {
        pool.remove(chainId)?.apply { finish() }
        releaseLifecycleRef(chainId)
    }

    private suspend fun acquireLifecycleRefIfForeground(chain: Chain) {
        if (appLifecycleObserver.getCurrentState() == AppLifecycleState.FOREGROUND) {
            acquireLifecycleRef(chain.id)
        }
    }

    private suspend fun acquireLifecycleRef(chainId: ChainId) {
        if (lifecycleRefs.containsKey(chainId)) return
        val ref = chainConnectionRefCounter.requestConnectionEnabled(chainId, "AppLifecycle")
        lifecycleRefs[chainId] = ref
    }

    private suspend fun acquireAllLifecycleRefs() {
        pool.keys.forEach { chainId -> acquireLifecycleRef(chainId) }
    }

    private fun releaseLifecycleRef(chainId: ChainId) {
        lifecycleRefs.remove(chainId)?.let { ref ->
            scope.launch { ref.release() }
        }
    }

    private fun releaseAllLifecycleRefs() {
        lifecycleRefs.keys.toList().forEach { chainId -> releaseLifecycleRef(chainId) }
    }
}
