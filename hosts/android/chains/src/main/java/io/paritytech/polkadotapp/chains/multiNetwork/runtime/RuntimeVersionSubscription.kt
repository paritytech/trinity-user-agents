package io.paritytech.polkadotapp.chains.multiNetwork.runtime

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.chain.runtimeVersionChange
import io.novasama.substrate_sdk_android.wsrpc.subscriptionFlow
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnection
import io.paritytech.polkadotapp.database.dao.ChainDao
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import timber.log.Timber

class RuntimeVersionSubscription(
    private val chainId: String,
    connection: ChainConnection,
    private val chainDao: ChainDao,
    private val runtimeSyncService: RuntimeSyncService,
) : CoroutineScope by CoroutineScope(Dispatchers.Default) {
    init {
        connection.socketService.subscriptionFlow(SubscribeRuntimeVersionRequest)
            .map { it.runtimeVersionChange() }
            .onEach { runtimeVersion ->
                chainDao.updateRemoteRuntimeVersionIfChainExists(
                    chainId,
                    runtimeVersion = runtimeVersion.specVersion,
                    transactionVersion = runtimeVersion.transactionVersion
                )

                runtimeSyncService.applyRuntimeVersion(chainId)
            }
            .catch { Timber.e("Failed to sync runtime version for $chainId", it) }
            .launchIn(this)
    }
}
