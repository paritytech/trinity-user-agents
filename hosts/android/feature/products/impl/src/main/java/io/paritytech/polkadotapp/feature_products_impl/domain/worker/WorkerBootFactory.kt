package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import dagger.Lazy
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.HostApiProductsScriptExecutor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIChatWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import kotlinx.coroutines.CoroutineScope
import javax.inject.Inject

/**
 * Builds and starts a product's headless worker. Injected into [ProductWorkerRefCounter] once at
 * startup so the ref counter can be exercised without a real runtime, and so the product dependency
 * graph is assembled before any worker boots.
 */
interface WorkerBootFactory {
    suspend fun boot(
        productId: ProductId,
        botApi: BindableProductsBotApi,
        scope: CoroutineScope,
    ): ProductWorker
}

class RealWorkerBootFactory @Inject constructor(
    private val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory,
    private val runtimeSettings: ProductRuntimeSettings,
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    private val workerSupervisor: Lazy<TrUAPIWorkerSupervisor>,
) : WorkerBootFactory {
    override suspend fun boot(
        productId: ProductId,
        botApi: BindableProductsBotApi,
        scope: CoroutineScope,
    ): ProductWorker {
        return if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            bootCore(productId, botApi, scope)
        } else {
            bootJs(productId, botApi, scope)
        }
    }

    private suspend fun bootCore(
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

    private suspend fun bootJs(
        productId: ProductId,
        botApi: BindableProductsBotApi,
        scope: CoroutineScope,
    ): ProductWorker {
        val executor = scriptExecutorFactory.create(productId)
        return executor.initializeBot(botApi, scope).map { executor }.getOrThrow()
    }
}
