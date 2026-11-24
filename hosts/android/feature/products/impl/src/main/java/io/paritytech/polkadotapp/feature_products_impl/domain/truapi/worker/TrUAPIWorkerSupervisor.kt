package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import dagger.Lazy
import io.parity.truapi.TrUAPIProductExecution
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
import kotlinx.coroutines.withTimeout
import timber.log.Timber
import uniffi.truapi.NativeHostWebViewWorker
import uniffi.truapi.NativeWorkerAttempt
import uniffi.truapi.ProductId
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

/** Creates and disposes each worker's hidden WebView on the main thread. */
class TrUAPIWorkerSupervisor @Inject constructor(
    private val hostBridgeFactory: ProductTrUAPIHostBridge.Factory,
    private val chainDirectory: TrUAPIChainDirectory,
    private val scriptResolver: ProductScriptResolver,
    private val webViewProviderFactory: ChatWebViewProvider.Factory,
    private val bootstrapInstaller: TrUAPIBootstrapInstaller,
    private val refCounter: Lazy<ProductWorkerRefCounter>,
    private val navigation: ChatWorkerNavigation,
    dispatchers: CoroutineDispatchers,
) : NativeHostWebViewWorker {
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
                    val execution = boot(productId, attempt, worker)
                    if (!execution.reportWorkerModuleStarted()) dispose(id)
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

    private suspend fun boot(productId: ProductId, attempt: NativeWorkerAttempt, worker: WebViewWorker): TrUAPIProductExecution {
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
        withTimeout(READY_TIMEOUT) {
            webViewRuntime.waitForReady()
            webViewRuntime.startWorkerModule(workerScript.entrypoint)
        }
        return execution
    }

    private fun dispose(id: ULong) {
        val worker = workers.remove(id) ?: return
        worker.scope.cancel()
        worker.runtime?.dispose()
    }

    companion object {
        // Generous: a cold WebView on a slow device fetching a worker archive over dotNS.
        private val READY_TIMEOUT = 60.seconds

        val EXECUTION_WAIT_TIMEOUT = READY_TIMEOUT + 30.seconds
    }
}
