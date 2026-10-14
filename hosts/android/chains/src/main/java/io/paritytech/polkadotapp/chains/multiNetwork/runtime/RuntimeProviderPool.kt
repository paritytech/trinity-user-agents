package io.paritytech.polkadotapp.chains.multiNetwork.runtime

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.types.BaseTypeSynchronizer
import io.paritytech.polkadotapp.chains.util.typesUsage
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RuntimeProviderPool @Inject constructor(
    private val runtimeFactory: RuntimeFactory,
    private val runtimeSyncService: RuntimeSyncService,
    private val baseTypeSynchronizer: BaseTypeSynchronizer,
) {
    private val pool = ConcurrentHashMap<String, RuntimeProvider>()

    fun getRuntimeProvider(chainId: String) = pool.getValue(chainId)

    fun setupRuntimeProvider(chain: Chain): RuntimeProvider {
        val provider = pool.getOrPut(chain.id) {
            RuntimeProvider(runtimeFactory, runtimeSyncService, baseTypeSynchronizer, chain)
        }

        provider.considerUpdatingTypesUsage(chain.typesUsage)

        return provider
    }

    fun removeRuntimeProvider(chainId: String) {
        pool.remove(chainId)?.apply { finish() }
    }
}
