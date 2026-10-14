package io.paritytech.polkadotapp.chains.multiNetwork.requests

import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.storage.StorageSubscriptionMultiplexer
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.storage.subscribeUsing
import io.novasama.substrate_sdk_android.wsrpc.subscribe
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getSocketOrNull
import io.paritytech.polkadotapp.chains.storage.StorageChange
import io.paritytech.polkadotapp.common.utils.invokeOnCompletion
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlin.coroutines.CoroutineContext

class StorageSharedRequestsBuilderFactory(
    private val chainRegistry: ChainRegistry,
) {
    suspend fun create(
        chainId: ChainId,
    ): StorageSharedRequestsBuilder {
        val substrateProxy = StorageSubscriptionMultiplexer.Builder()

        val rpcSocket = chainRegistry.getSocketOrNull(chainId)

        return StorageSharedRequestsBuilder(
            socketService = rpcSocket,
            substrateProxy = substrateProxy,
        )
    }
}

suspend inline fun <T> StorageSharedRequestsBuilderFactory.withSubscription(
    coroutineScope: CoroutineScope,
    chainId: ChainId,
    builderAction: (StorageSharedRequestsBuilder) -> T
): T {
    val builder = create(chainId)
    return builderAction(builder).also {
        builder.subscribe(coroutineScope)
    }
}

context(scope: CoroutineScope)
suspend inline fun <T> StorageSharedRequestsBuilderFactory.withSubscription(
    chainId: ChainId,
    builderAction: (StorageSharedRequestsBuilder) -> T
): T {
    return withSubscription(scope, chainId, builderAction)
}

class StorageSharedRequestsBuilder(
    override val socketService: SocketService?,
    private val substrateProxy: StorageSubscriptionMultiplexer.Builder,
) : SharedRequestsBuilder {
    override fun subscribe(key: String): Flow<StorageChange> {
        return substrateProxy.subscribe(key)
            .map { StorageChange(it.block, it.key, it.value) }
    }

    fun subscribe(coroutineScope: CoroutineScope) {
        val cancellable = socketService?.subscribeUsing(substrateProxy.build())

        if (cancellable != null) {
            coroutineScope.invokeOnCompletion { cancellable.cancel() }
        }
    }
}

fun StorageSharedRequestsBuilder.subscribe(coroutineContext: CoroutineContext) {
    subscribe(CoroutineScope(coroutineContext))
}
