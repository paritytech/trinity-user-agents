package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import dagger.Lazy
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIChatWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import kotlinx.coroutines.CoroutineScope
import uniffi.truapi.ProductId
import javax.inject.Inject

/**
 * Builds and starts a product's headless worker. Injected into [ProductWorkerRefCounter] once at
 * startup so the ref counter can be exercised without a real runtime, and so the product dependency
 * graph is assembled before any worker boots.
 */
interface WorkerBootFactory {
    /**
     * Boots the worker for [productId] on [scope], wiring [botApi] as its host-call surface.
     * Throws when the core cannot produce one. A product with no worker script still boots here and
     * surfaces as a supervisor `Failed` state later.
     */
    suspend fun boot(
        productId: ProductId,
        botApi: BindableProductsBotApi,
        scope: CoroutineScope,
    ): ProductWorker
}

class RealWorkerBootFactory @Inject constructor(
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    private val workerSupervisor: Lazy<TrUAPIWorkerSupervisor>,
) : WorkerBootFactory {
    override suspend fun boot(
        productId: ProductId,
        botApi: BindableProductsBotApi,
        scope: CoroutineScope,
    ): ProductWorker = TrUAPIChatWorker(
        productId = productId,
        runtime = runtimeProvider.get().runtime().getOrThrow(),
        workers = workerSupervisor.get(),
        chatMessaging = botApi,
        scope = scope,
    )
}
