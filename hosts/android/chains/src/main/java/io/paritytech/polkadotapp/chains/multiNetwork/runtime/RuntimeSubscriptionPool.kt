package io.paritytech.polkadotapp.chains.multiNetwork.runtime

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnection
import io.paritytech.polkadotapp.database.dao.ChainDao
import kotlinx.coroutines.cancel
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RuntimeSubscriptionPool @Inject constructor(
    private val chainDao: ChainDao,
    private val runtimeSyncService: RuntimeSyncService,
) {
    private val pool = ConcurrentHashMap<String, RuntimeVersionSubscription>()

    fun setupRuntimeSubscription(
        chain: Chain,
        connection: ChainConnection,
    ): RuntimeVersionSubscription {
        return pool.getOrPut(chain.id) {
            RuntimeVersionSubscription(chain.id, connection, chainDao, runtimeSyncService)
        }
    }

    fun removeSubscription(chainId: String) {
        pool.remove(chainId)?.apply { cancel() }
    }
}
