package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import dagger.Lazy
import io.parity.truapi.ChatHostBridge
import io.parity.truapi.LocalhostBridgeBootstrap
import io.parity.truapi.PocketHostBridge
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.parity.truapi.WorkerEngineHost
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.WebViewRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketCardStore
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.WorkerScript
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductPocketHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIBootstrapInstaller
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import timber.log.Timber
import uniffi.truapi.WorkerBundle
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.seconds

@Singleton
class TrUAPIWorkerSupervisor @Inject constructor(
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    private val bundles: TrUAPIWorkerBundles,
    private val webViewProviderFactory: ChatWebViewProvider.Factory,
    private val bootstrapInstaller: TrUAPIBootstrapInstaller,
    private val pocketStore: PocketCardStore,
    private val dispatchers: CoroutineDispatchers,
) : WorkerEngineHost {
    private class RunningWorker(val scope: CoroutineScope, val execution: TrUAPIProductExecution) {
        var webViewRuntime: WebViewRuntime? = null
    }

    @Volatile private var host: TrUAPIHostRuntime? = null

    fun attach(runtime: TrUAPIHostRuntime) { host = runtime }

    private val scope = CoroutineScope(SupervisorJob() + dispatchers.main)
    private val workers = ConcurrentHashMap<ProductId, RunningWorker>()
    private val chats = ConcurrentHashMap<String, TrUAPIWorkerChat>()
    private val pockets = ConcurrentHashMap<String, ProductPocketHostBridge>()
    private val executions = MutableStateFlow<Map<ProductId, TrUAPIProductExecution>>(emptyMap())

    fun execution(productId: ProductId): Flow<TrUAPIProductExecution?> = executions.map { it[productId] }.distinctUntilChanged()
    fun currentExecution(productId: ProductId): TrUAPIProductExecution? = executions.value[productId]
    fun chat(productId: ProductId): TrUAPIWorkerChat = chats.computeIfAbsent(productId.value) { TrUAPIWorkerChat() }

    override fun chatBridge(productId: String): ChatHostBridge = chat(ProductId.fromStoredValue(productId))
    override fun pocketBridge(productId: String): PocketHostBridge = pockets.computeIfAbsent(productId) {
        ProductPocketHostBridge(ProductId.fromStoredValue(it), pocketStore, scope)
    }

    override suspend fun fetchWorkerBundle(productId: String, contentHash: ByteArray?): WorkerBundle {
        val override = if (contentHash == null) checkNotNull(host).products().firstOrNull { it.productId == productId }?.workerUrlOverride else null
        return bundles.fetch(productId, contentHash, override)
    }

    override fun startWorker(productId: String, execution: TrUAPIProductExecution, bundle: WorkerBundle) {
        val product = ProductId.fromStoredValue(productId)
        val worker = RunningWorker(scope.childScope(), execution)
        check(workers.putIfAbsent(product, worker) == null) { "Core started an already running worker" }
        worker.scope.launch {
            runCatching { boot(product, worker, bundle) }.onFailure { error ->
                if (error is kotlinx.coroutines.CancellationException) throw error
                Timber.e(error, "Rust worker engine failed for %s", productId)
                runtimeProvider.get().runtime().getOrThrow().notifyWorkerFailed(execution, error.message.orEmpty())
            }
        }
    }

    override suspend fun stopWorker(productId: String) {
        val product = ProductId.fromStoredValue(productId)
        val worker = workers.remove(product) ?: return
        executions.update { it - product }
        pockets.remove(productId)?.stop()
        worker.scope.coroutineContext[Job]?.cancelAndJoin()
        withContext(dispatchers.main) { worker.webViewRuntime?.dispose() }
    }

    suspend fun stopAll() {
        workers.keys.toList().forEach { stopWorker(it.value) }
        chats.clear()
        pockets.clear()
        host = null
    }

    private suspend fun boot(productId: ProductId, worker: RunningWorker, bundle: WorkerBundle) {
        val script = WorkerScript.of(Json.parseToJsonElement(bundle.manifest.decodeToString()).jsonObject.getValue("scriptUrl").jsonPrimitive.content)
        val provider = webViewProviderFactory.create(ChatWebViewConfig(productId, script, File(bundle.localPath)), worker.scope)
        val engine = WebViewRuntime(provider).also { worker.webViewRuntime = it }
        engine.initialize()
        val endpoint = worker.execution.startWsBridge()
        val bootstrap = LocalhostBridgeBootstrap.script(endpoint.port, endpoint.token)
        provider.addWebViewSetup(bootstrapInstaller.installerFor(setOf(script.baseUrl))(bootstrap))
        provider.useTrUAPIPermissions(worker.execution)
        provider.addOnWebViewDestroyedListener {
            worker.scope.launch {
                runtimeProvider.get().runtime().getOrThrow().notifyWorkerFailed(worker.execution, "WebView renderer exited")
            }
        }
        pockets[productId.value]?.start(worker.execution::notifyPocketCardsChanged)
        engine.loadInitialPage()
        withTimeout(60.seconds) { engine.waitForReady() }
        engine.loadEntryModule(script.entrypoint).getOrThrow()
        executions.update { it + (productId to worker.execution) }
    }
}
