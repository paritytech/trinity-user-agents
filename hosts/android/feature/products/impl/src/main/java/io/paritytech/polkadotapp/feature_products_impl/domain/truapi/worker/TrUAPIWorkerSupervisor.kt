package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import dagger.Lazy
import uniffi.truapi.ProductExecutionKind
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.WebViewRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.WorkerScript
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIBootstrapInstaller
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIChainDirectory
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withContext
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.seconds

enum class WorkerDemand { START, STOP }

/**
 * Runs product workers on the core for as long as the core's reference ledger wants them: a `Start`
 * boots the worker script in a hidden WebView behind a `WORKER` execution, a `Stop` tears it down.
 */
@Singleton
class TrUAPIWorkerSupervisor @Inject constructor(
    // Lazy breaks the Dagger cycle: the runtime's own bridge reports demand back into this supervisor.
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    private val hostBridgeFactory: ProductTrUAPIHostBridge.Factory,
    private val chainDirectory: TrUAPIChainDirectory,
    private val scriptResolver: ProductScriptResolver,
    private val webViewProviderFactory: ChatWebViewProvider.Factory,
    private val bootstrapInstaller: TrUAPIBootstrapInstaller,
    private val dispatchers: CoroutineDispatchers,
) {
    private class RunningWorker(val scope: CoroutineScope) {
        var webViewRuntime: WebViewRuntime? = null
    }

    // WebViews are created and driven on the main thread.
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.main)
    private val transitions = Mutex()
    private val workers = mutableMapOf<ProductId, RunningWorker>()
    private val demanded = mutableSetOf<ProductId>()
    private var suspended = false
    private val executions = MutableStateFlow<Map<ProductId, TrUAPIProductExecution>>(emptyMap())
    private val chats = ConcurrentHashMap<ProductId, TrUAPIWorkerChat>()

    fun chat(productId: ProductId): TrUAPIWorkerChat = chats.computeIfAbsent(productId) { TrUAPIWorkerChat() }

    suspend fun pause() = withContext(dispatchers.main) {
        transitions.withLock {
            suspended = true
            workers.keys.toList().forEach(::stop)
        }
    }

    suspend fun resume() = withContext(dispatchers.main) {
        transitions.withLock {
            suspended = false
            demanded.forEach(::start)
        }
    }

    suspend fun stopAll() = withContext(dispatchers.main) {
        transitions.withLock {
            workers.keys.toList().forEach(::stop)
            demanded.clear()
            chats.clear()
            suspended = false
        }
    }

    /** Demand crossing zero, as the core reports it. May arrive on any thread, so the work is handed to [scope]. */
    fun onDemandChanged(productId: ProductId, demand: WorkerDemand, isCurrentRuntime: () -> Boolean = { true }) {
        scope.launch {
            transitions.withLock {
                if (!isCurrentRuntime()) return@withLock
                when (demand) {
                    WorkerDemand.START -> {
                        demanded += productId
                        if (!suspended) start(productId)
                    }
                    WorkerDemand.STOP -> {
                        demanded -= productId
                        stop(productId)
                    }
                }
            }
        }
    }

    /** The product's worker execution while it runs, null otherwise. */
    fun execution(productId: ProductId): Flow<TrUAPIProductExecution?> =
        executions.map { it[productId] }.distinctUntilChanged()

    fun currentExecution(productId: ProductId): TrUAPIProductExecution? = executions.value[productId]

    private fun start(productId: ProductId) {
        if (productId in workers) return
        val worker = RunningWorker(scope.childScope())
        workers[productId] = worker
        worker.scope.launch {
            boot(productId, worker)
                .logFailure("TrUAPI worker for ${productId.value} failed to start")
                // A half-booted worker left in the map swallows every later start, and the core
                // keeps counting the reference the card holds, so no stop ever arrives to clear it.
                // Only this one: a stop and a new start may have overtaken the failure.
                .onFailure { transitions.withLock { if (workers[productId] === worker) stop(productId) } }
                .onSuccess { Timber.d("TrUAPI worker for %s is running", productId.value) }
        }
    }

    private suspend fun boot(productId: ProductId, worker: RunningWorker): Result<TrUAPIProductExecution> {
        val script = scriptResolver.resolveWorker(productId).getOrElse { return Result.failure(it) }
        val runtime = runtimeProvider.get().runtime().getOrElse { return Result.failure(it) }
        val workerScript = WorkerScript.of(script.scriptUrl)

        val provider = webViewProviderFactory.create(ChatWebViewConfig(productId, workerScript), worker.scope)
        val webViewRuntime = WebViewRuntime(provider).also { worker.webViewRuntime = it }
        // The bootstrap publishes the loopback port and token, so it goes to the worker's own origin only.
        val installBootstrap = runCatching {
            webViewRuntime.initialize()
            bootstrapInstaller.installerFor(setOf(workerScript.baseUrl))
        }.getOrElse { return Result.failure(it) }
        provider.addOnWebViewDestroyedListener { rebootAfterRendererLoss(productId, worker) }

        return hostBridgeFactory.create(worker.scope)
            .attach(
                runtime,
                productId,
                chainDirectory.resolve(),
                ignoredNavigation(),
                ProductExecutionKind.WORKER,
                chat = chat(productId),
            ) { bootstrap -> provider.addWebViewSetup(installBootstrap(bootstrap)) }
            .flatMap { execution ->
                runCatching {
                    provider.useTrUAPIPermissions(execution)
                    webViewRuntime.loadInitialPage()
                    // A page that never reports ready would otherwise hold the boot open forever,
                    // and with it the execution, the WebView and the card's static face.
                    withTimeout(READY_TIMEOUT) { webViewRuntime.waitForReady() }
                }
                    // The bootstrap runs at document start; the entry module loads once the page
                    // exists, so it connects.
                    .flatMap { webViewRuntime.loadEntryModule(workerScript.entrypoint) }
                    .map { execution }
            }
            .onSuccess { execution -> executions.update { it + (productId to execution) } }
    }

    // The renderer takes the running script with it, and only a fresh boot runs the script again.
    private fun rebootAfterRendererLoss(productId: ProductId, worker: RunningWorker) {
        scope.launch {
            transitions.withLock {
                if (workers[productId] !== worker || productId !in executions.value) return@withLock
                stop(productId)
                start(productId)
            }
        }
    }

    private fun stop(productId: ProductId) {
        val worker = workers.remove(productId) ?: return
        executions.update { it - productId }
        worker.webViewRuntime?.dispose()
        worker.scope.cancel()
    }

    // A worker has no screen of its own to navigate; a `navigate_to` from it is logged and dropped.
    private fun ignoredNavigation() = NavigationPolicy.DeeplinkNavigation(
        onDeeplinkNavigation = { Timber.d("Ignored navigation from a Pocket worker: %s", it) },
    )

    private companion object {
        // Generous: a cold WebView on a slow device fetching a worker archive over dotNS.
        val READY_TIMEOUT = 60.seconds
    }
}
