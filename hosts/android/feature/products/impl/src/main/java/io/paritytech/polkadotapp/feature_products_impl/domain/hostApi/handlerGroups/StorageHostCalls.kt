package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.feature_products_impl.data.storage.ProductLocalStorage
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map

class StorageHostCalls(
    private val productLocalStorage: ProductLocalStorage,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<LocalStorageReadParams, LocalStorageReadResult>("localStorageRead") { params ->
            runCatching {
                val value = productLocalStorage.read(callingProductIdProvider.getProductId().getOrThrow(), params.key)
                LocalStorageReadResult(value)
            }
        }

        bridge.registerHandler<LocalStorageWriteParams, Unit>("localStorageWrite") { params ->
            runCatching {
                productLocalStorage.write(callingProductIdProvider.getProductId().getOrThrow(), params.key, params.value)
            }
        }

        bridge.registerHandler<LocalStorageClearParams, Unit>("localStorageClear") { params ->
            runCatching {
                productLocalStorage.clear(callingProductIdProvider.getProductId().getOrThrow(), params.key)
            }
        }

        bridge.registerSubscription<LocalStorageSubscribeParams, LocalStorageSubscribeResult>("localStorageSubscribe") { params ->
            flow {
                callingProductIdProvider.getProductId().fold(
                    onSuccess = { productId ->
                        emitAll(productLocalStorage.observe(productId, params.key).map { LocalStorageSubscribeResult(it) })
                    },
                    // Surfaces via the subscription's onCompletion → host-api error, not a raw throw to JS.
                    onFailure = { error -> throw error },
                )
            }
        }
    }
}

private data class LocalStorageReadParams(val key: String)
private data class LocalStorageReadResult(val value: String?)
private data class LocalStorageWriteParams(val key: String, val value: String)
private data class LocalStorageClearParams(val key: String)
private data class LocalStorageSubscribeParams(val key: String)
private data class LocalStorageSubscribeResult(val value: String?)
