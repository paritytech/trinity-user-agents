package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import dagger.Lazy
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.WebViewRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.WorkerScript
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductChatHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIBootstrapInstaller
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIChainDirectory
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ChatWorkerNavigation
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import timber.log.Timber
import uniffi.truapi.NativeWorkerEngine
import uniffi.truapi.NativeWorkerAttempt
import uniffi.truapi.ProductId
import javax.inject.Inject

/** Creates and disposes each worker's hidden WebView on the main thread. */
class WebViewWorkerHost @Inject constructor(
    private val hostBridgeFactory: ProductTrUAPIHostBridge.Factory,
    private val chainDirectory: TrUAPIChainDirectory,
    private val scriptResolver: ProductScriptResolver,
    private val webViewProviderFactory: ChatWebViewProvider.Factory,
    private val bootstrapInstaller: TrUAPIBootstrapInstaller,
    private val refCounter: Lazy<ProductWorkerRefCounter>,
    private val navigation: ChatWorkerNavigation,
    dispatchers: CoroutineDispatchers,
) : NativeWorkerEngine {
    private class WebViewWorker(val scope: CoroutineScope) {
        var runtime: WebViewRuntime? = null
    }

    private val scope = CoroutineScope(SupervisorJob() + dispatchers.main)
    private val workers = mutableMapOf<ULong, WebViewWorker>()

    override fun start(productId: ProductId, attempt: NativeWorkerAttempt) {
        scope.launch {
            val id = attempt.id()
            val worker = WebViewWorker(scope.childScope())
            workers[id] = worker
            worker.scope.launch {
                try {
                    boot(productId, attempt, worker)
                    if (!attempt.moduleStarted()) dispose(id)
                } catch (cause: Exception) {
                    ensureActive()
                    Timber.w(cause, "TrUAPI worker for %s failed to start", productId.value)
                    attempt.failed(cause.message ?: cause.toString())
                    dispose(id)
                }
            }
        }
    }

    override fun stop(productId: ProductId, attemptId: ULong) {
        scope.launch { dispose(attemptId) }
    }

    private suspend fun boot(productId: ProductId, attempt: NativeWorkerAttempt, worker: WebViewWorker) {
        val script = scriptResolver.resolveWorker(productId).getOrThrow()
        val workerScript = WorkerScript.of(script.scriptUrl)
        val provider = webViewProviderFactory.create(ChatWebViewConfig(productId, workerScript), worker.scope)
        val webViewRuntime = WebViewRuntime(provider)
        worker.runtime = webViewRuntime
        webViewRuntime.initialize()
        val installBootstrap = bootstrapInstaller.installerFor(setOf(workerScript.baseUrl), null)
        provider.addOnWebViewDestroyedListener {
            if (worker.scope.isActive) attempt.failed("WebView renderer exited")
        }
        val chat = if (script.includesChat) ProductChatHostBridge(productId, refCounter.get().chatMessaging(productId)) else null
        val execution = hostBridgeFactory.create(worker.scope).attachWorker(
            attempt,
            productId,
            chainDirectory.resolve(),
            navigation.policy(productId, worker.scope),
            chat,
        ) { bootstrap -> provider.addWebViewSetup(installBootstrap(bootstrap)) }.getOrThrow()
        provider.useTrUAPIPermissions(execution)
        webViewRuntime.loadInitialPage()
        webViewRuntime.waitForReady()
        webViewRuntime.startWorkerModule(workerScript.entrypoint)
    }

    private fun dispose(id: ULong) {
        val worker = workers.remove(id) ?: return
        worker.scope.cancel()
        worker.runtime?.dispose()
    }
}
