package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import dagger.Lazy
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadTrigger
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.WebViewRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductHasNoWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.WorkerScript
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductChatHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIBootstrapInstaller
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIChainDirectory
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ChatWorkerNavigation
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import timber.log.Timber
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.seconds

enum class WorkerDemand { START, STOP }

sealed interface WorkerExecutionState {
    /** [sessionId] is this activation's host session, which its own telemetry carries too. */
    data class Running(val execution: TrUAPIProductExecution, val sessionId: String? = null) : WorkerExecutionState
    data class Failed(val reason: Throwable, val sessionId: String? = null) : WorkerExecutionState
}

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
    private val refCounter: Lazy<ProductWorkerRefCounter>,
    private val navigation: ChatWorkerNavigation,
    private val bootTelemetry: HostWorkerBootTelemetry,
    dispatchers: CoroutineDispatchers,
) {
    private class RunningWorker(val scope: CoroutineScope, val boot: HostWorkerBootTelemetry.WorkerBoot) {
        val sessionId get() = boot.sessionId
        var webViewRuntime: WebViewRuntime? = null
    }

    // WebViews are created and driven on the main thread.
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.main)
    private val transitions = Mutex()
    private val workers = mutableMapOf<ProductId, RunningWorker>()
    private val executionStates = MutableStateFlow<Map<ProductId, WorkerExecutionState>>(emptyMap())

    // Read off the main thread, by the faces waiting on a worker still booting.
    private val bootingSessions = ConcurrentHashMap<ProductId, String>()

    /** Demand crossing zero, as the core reports it. May arrive on any thread, so the work is handed to [scope]. */
    fun onDemandChanged(productId: ProductId, demand: WorkerDemand) {
        val demandedAtMillis = bootTelemetry.nowMillis()
        scope.launch {
            transitions.withLock {
                when (demand) {
                    WorkerDemand.START -> start(productId, demandedAtMillis)
                    WorkerDemand.STOP -> stop(productId)
                }
            }
        }
    }

    /** The product's worker execution while it runs, null otherwise. */
    fun execution(productId: ProductId): Flow<TrUAPIProductExecution?> =
        executionState(productId).map { (it as? WorkerExecutionState.Running)?.execution }.distinctUntilChanged()

    fun currentExecution(productId: ProductId): TrUAPIProductExecution? =
        (executionStates.value[productId] as? WorkerExecutionState.Running)?.execution

    fun executionState(productId: ProductId): Flow<WorkerExecutionState?> =
        executionStates.map { it[productId] }.distinctUntilChanged()

    /**
     * The host session of the product's worker activation, minted when it started, which its face and
     * chain connects report under: while it boots, runs, or after its boot failed.
     */
    fun sessionId(productId: ProductId): String? = when (val state = executionStates.value[productId]) {
        is WorkerExecutionState.Running -> state.sessionId
        is WorkerExecutionState.Failed -> state.sessionId
        null -> bootingSessions[productId]
    }

    private fun start(productId: ProductId, demandedAtMillis: Long) {
        if (productId in workers) return
        // Each activation is a session of its own: the worker's spans join its host events by it.
        val bootReport = bootTelemetry.begin(productId, UUID.randomUUID().toString(), demandedAtMillis)
        val worker = RunningWorker(scope.childScope(), bootReport)
        workers[productId] = worker
        bootingSessions[productId] = worker.sessionId
        executionStates.update { it - productId }
        worker.scope.launch {
            val booted = try {
                withContext(HostLoadContext(HostLoadTrigger.OPEN, worker.sessionId)) { boot(productId, worker, bootReport) }
            } catch (cancelled: CancellationException) {
                bootReport.finish(HostEventOutcome.ABANDONED, bootTelemetry.nowMillis())
                throw cancelled
            }
            val endedAtMillis = bootTelemetry.nowMillis()
            booted
                .logFailure("TrUAPI worker for ${productId.value} failed to start")
                // A half-booted worker left in the map swallows every later start, and the core
                // keeps counting the reference the card holds, so no stop ever arrives to clear it.
                // Only this one: a stop and a new start may have overtaken the failure.
                .onFailure { cause ->
                    bootReport.finish(bootFailureOutcome(cause), endedAtMillis)
                    transitions.withLock {
                        if (workers[productId] === worker) {
                            stop(productId)
                            executionStates.update {
                                it + (productId to WorkerExecutionState.Failed(cause, worker.sessionId))
                            }
                        }
                    }
                }
                .onSuccess { execution ->
                    transitions.withLock {
                        if (workers[productId] === worker) {
                            bootReport.finish(HostEventOutcome.OK, endedAtMillis)
                            executionStates.update {
                                it + (productId to WorkerExecutionState.Running(execution, worker.sessionId))
                            }
                            Timber.d("TrUAPI worker for %s is running", productId.value)
                        } else {
                            bootReport.finish(HostEventOutcome.ABANDONED, endedAtMillis)
                        }
                    }
                }
        }
    }

    // The boot's own steps catch what they throw, a cancellation included, so a stop mid-boot
    // arrives here as a failure too.
    private fun CoroutineScope.bootFailureOutcome(cause: Throwable): HostEventOutcome = when {
        cause is ProductHasNoWorker -> HostEventOutcome.NO_WORKER
        cause is TimeoutCancellationException -> HostEventOutcome.TIMEOUT
        cause is CancellationException || !isActive -> HostEventOutcome.ABANDONED
        else -> HostEventOutcome.ERROR
    }

    private suspend fun boot(
        productId: ProductId,
        worker: RunningWorker,
        report: HostWorkerBootTelemetry.WorkerBoot,
    ): Result<TrUAPIProductExecution> {
        val resolveStartedAtMillis = bootTelemetry.nowMillis()
        val script = scriptResolver.resolveWorker(productId).getOrElse { return Result.failure(it) }
        report.phase(HostWorkerBootTelemetry.RESOLVE_MS, resolveStartedAtMillis)
        val runtime = runtimeProvider.get().runtime().getOrElse { return Result.failure(it) }
        val workerScript = WorkerScript.of(script.scriptUrl)

        val hostLoad = HostLoadContext(HostLoadTrigger.OPEN, worker.sessionId)
        val provider = webViewProviderFactory.create(ChatWebViewConfig(productId, workerScript, hostLoad), worker.scope)
        val webViewRuntime = WebViewRuntime(provider).also { worker.webViewRuntime = it }
        // The bootstrap publishes the loopback port and token, so it goes to the worker's own origin only.
        val installBootstrap = runCatching {
            webViewRuntime.initialize()
            bootstrapInstaller.installerFor(setOf(workerScript.baseUrl), worker.sessionId)
        }.getOrElse { return Result.failure(it) }
        provider.addOnWebViewDestroyedListener { rebootAfterRendererLoss(productId, worker) }

        val executionOpenStartedAtMillis = bootTelemetry.nowMillis()
        return hostBridgeFactory.create(worker.scope)
            .attach(
                runtime,
                productId,
                chainDirectory.resolve(),
                navigation.policy(productId, worker.scope),
                ExecutableKind.WORKER,
                chat = if (script.includesChat) {
                    ProductChatHostBridge(productId, refCounter.get().chatMessaging(productId))
                } else {
                    null
                },
                hostSessionId = worker.sessionId,
            ) { bootstrap -> provider.addWebViewSetup(installBootstrap(bootstrap)) }
            .onSuccess { report.phase(HostWorkerBootTelemetry.EXECUTION_OPEN_MS, executionOpenStartedAtMillis) }
            .flatMap { execution ->
                runCatching {
                    provider.useTrUAPIPermissions(execution)
                    // The WebView is built by the load below, and its setups run as soon as it exists.
                    val loadStartedAtMillis = bootTelemetry.nowMillis()
                    provider.addWebViewSetup { report.webViewCreated(loadStartedAtMillis) }
                    webViewRuntime.loadInitialPage()
                    // A page that never reports ready would otherwise hold the boot open forever,
                    // and with it the execution, the WebView and the card's static face.
                    withTimeout(READY_TIMEOUT) { webViewRuntime.waitForReady() }
                    report.blankReady()
                }
                    // The bootstrap runs at document start; the entry module loads once the page
                    // exists, so it connects.
                    .flatMap { webViewRuntime.loadEntryModule(workerScript.entrypoint) }
                    .map { execution }
            }
    }

    // The renderer takes the running script with it, and only a fresh boot runs the script again.
    private fun rebootAfterRendererLoss(productId: ProductId, worker: RunningWorker) {
        scope.launch {
            transitions.withLock {
                if (workers[productId] !== worker || currentExecution(productId) == null) return@withLock
                stop(productId)
                start(productId, demandedAtMillis = bootTelemetry.nowMillis())
            }
        }
    }

    private fun stop(productId: ProductId) {
        executionStates.update { it - productId }
        bootingSessions.remove(productId)
        val worker = workers.remove(productId) ?: return
        worker.webViewRuntime?.dispose()
        worker.scope.cancel()
        worker.boot.ended()
    }

    companion object {
        // Generous: a cold WebView on a slow device fetching a worker archive over dotNS.
        private val READY_TIMEOUT = 60.seconds

        val EXECUTION_WAIT_TIMEOUT = READY_TIMEOUT + 30.seconds
    }
}
