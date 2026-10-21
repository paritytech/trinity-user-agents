package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import dagger.Lazy
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.WorkerKeepAlive
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.HostApiProductsScriptExecutor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIChatWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.job
import javax.inject.Inject

/**
 * Builds and starts a product's headless worker. Injected into [ProductWorkerRefCounter] once at
 * startup so the ref counter can be exercised without a real runtime, and so the product dependency
 * graph is assembled before any worker boots.
 */
interface WorkerBootFactory {
    /**
     * Boots the worker for [productId] on [scope], wiring [botApi] as its host-call surface.
     * Throws when the chosen runtime cannot produce one: a product with no worker script fails here
     * on the JS path, but boots on the core path and surfaces as a supervisor `Failed` state later.
     */
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
    private val scriptResolver: ProductScriptResolver,
) : WorkerBootFactory {
    override suspend fun boot(
        productId: ProductId,
        botApi: BindableProductsBotApi,
        scope: CoroutineScope,
    ): ProductWorker {
        // The chat product needs the core's PersonChat, which the native host does not serve.
        return if (runtimeSettings.isTrUAPIRuntimeEnabled() || ReservedProductIds.isChat(productId)) {
            bootCore(productId, botApi, scope)
        } else {
            bootJs(productId, botApi, scope)
        }
    }

    private suspend fun bootCore(
        productId: ProductId,
        botApi: BindableProductsBotApi,
        scope: CoroutineScope,
    ): ProductWorker {
        val runtime = runtimeProvider.get().runtime().getOrThrow()
        val worker = TrUAPIChatWorker(
            productId = productId,
            runtime = runtime,
            workers = workerSupervisor.get(),
            chatMessaging = botApi,
            scope = scope,
        )
        declareForegroundWorker(productId, runtime, scope)
        return worker
    }

    // A product whose manifest declares a foreground worker keeps it while the app is in the foreground, also with
    // no room open, for as long as the product is booted (VERBOSITY-WASM-PRODUCT A2).
    private suspend fun declareForegroundWorker(productId: ProductId, runtime: TrUAPIHostRuntime, scope: CoroutineScope) {
        val keepAlive = scriptResolver.resolveWorker(productId).getOrNull()?.keepAlive
        if (keepAlive != WorkerKeepAlive.FOREGROUND) return
        val workers = workerSupervisor.get()
        workers.declareForegroundWorker(runtime, productId)
        scope.coroutineContext.job.invokeOnCompletion { workers.withdrawForegroundWorker(runtime, productId) }
    }

    private suspend fun bootJs(
        productId: ProductId,
        botApi: BindableProductsBotApi,
        scope: CoroutineScope,
    ): ProductWorker {
        val executor = scriptExecutorFactory.create(productId)
        return executor.initializeBot(botApi, scope).map { executor }.getOrThrow()
    }
}
