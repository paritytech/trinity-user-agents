package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import dagger.Lazy
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.HostBridge
import io.parity.truapi.HostSecretStorage
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.WebSocketChainProvider
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.di.TrUAPIChainHttpClient
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.WorkerDemand
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.OkHttpClient
import timber.log.Timber
import uniffi.truapi.AuthState
import uniffi.truapi.HostChainSet
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostNavigateToException
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.PairedHostRecord
import uniffi.truapi.PairedSsoPeer
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.RemotePermission
import uniffi.truapi.ResponderExit
import uniffi.truapi.UserConfirmationReview
import uniffi.truapi.WorkerTransition
import java.util.concurrent.atomic.AtomicReference
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Vends the one process-wide [TrUAPIHostRuntime]. Product executions open off
 * it and share its authentication session and core services, so it is built
 * lazily on first use and kept for the life of the process.
 */
@Singleton
class TrUAPIHostRuntimeProvider @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
    private val chainDirectory: TrUAPIChainDirectory,
    private val localSessionSource: TrUAPILocalSessionSource,
    private val accountRepository: AccountRepository,
    private val dotNsTldProvider: DotNsTldProvider,
    private val secretStorage: TrUAPISecretStorage,
    private val notifications: TrUAPINotifications,
    private val hostApiInteractor: Lazy<io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor>,
    @param:TrUAPIChainHttpClient private val chainHttpClient: OkHttpClient,
    private val confirmationLauncher: TrUAPIConfirmationLauncher,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val contactsBridge: AppContactsHostBridge,
    // Lazy: the supervisor boots workers on this runtime, and reports back through this bridge.
    private val workerSupervisor: Lazy<TrUAPIWorkerSupervisor>,
    dispatchers: CoroutineDispatchers,
) {
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.computation)
    private val bootMutex = Mutex()
    private class BoundRuntime(val runtime: TrUAPIHostRuntime, val walletId: String, val generation: Long)
    private var boot: Deferred<Result<BoundRuntime>>? = null

    private var runtimeScope: CoroutineScope? = null
    private val peerJobs = java.util.concurrent.ConcurrentHashMap<String, Job>()
    private val peerMutex = Mutex()
    private val recordRevision = MutableStateFlow(0L)
    val recordsChanged: StateFlow<Long> = recordRevision.asStateFlow()
    fun notifyRecordsChanged() { recordRevision.update { it + 1 } }
    private val authGeneration = java.util.concurrent.atomic.AtomicLong()
    private val workerRuntimeGeneration = java.util.concurrent.atomic.AtomicLong()
    private val pairedRecords = MutableStateFlow<List<PairedHostRecord>>(emptyList())
    val pairedHosts: StateFlow<List<PairedHostRecord>> = pairedRecords.asStateFlow()
    private val authState = MutableStateFlow<AuthState>(AuthState.Disconnected)

    /** Core-owned session state. Nothing consumes it yet; see [HostBridge.authStateChanged]. */
    val sessionState: StateFlow<AuthState> = authState.asStateFlow()

    // Resolved once when the runtime boots: the core asks for chains on its
    // dispatcher thread, where a suspending registry lookup is not allowed.
    private val cachedChains = AtomicReference(EMPTY_CHAINS)

    private val chainProvider = WebSocketChainProvider(
        resolver = { genesisHash -> cachedChains.get().endpoints[genesisHash.hexKey()].orEmpty() },
        client = chainHttpClient,
        onLog = { Timber.tag("truapi.chain").d("%s", it) },
    )

    /**
     * Boots in the provider's own scope, not the caller's: a product closing
     * mid-boot must not abandon a half-built runtime, which would leak the
     * native handle and let the next product boot a second one. A failed boot
     * is forgotten so the next caller retries.
     */
    suspend fun start() { runtime().getOrThrow() }

    suspend fun runtime(): Result<TrUAPIHostRuntime> {
        val pending = bootMutex.withLock {
            boot ?: scope.async { build().onSuccess(::wire) }.also { boot = it }
        }
        return pending.await().map { it.runtime }.onFailure {
            bootMutex.withLock { if (boot === pending) boot = null }
        }
    }

    fun <T> observeRecords(empty: T, read: suspend (TrUAPIHostRuntime) -> T): Flow<T> =
        combine(recordsChanged, sessionState) { _, state ->
            if (state !is AuthState.Connected) empty
            else {
                val generation = authGeneration.get()
                runCatching { read(runtime().getOrThrow()) }
                    .fold(
                        onSuccess = { if (generation == authGeneration.get()) it else empty },
                        onFailure = { if (generation != authGeneration.get()) empty else throw it },
                    )
            }
        }.onStart { start() }

    private suspend fun build(): Result<BoundRuntime> = runCatching {
        check(!context.getSystemService(android.app.KeyguardManager::class.java).isDeviceLocked) { "Unlock the device before activating the wallet" }
        val config = buildRuntimeConfig()
        cachedChains.set(chainDirectory.resolve())
        val session = localSessionSource.resolve().getOrThrow()
        val generation = workerRuntimeGeneration.incrementAndGet()
        val runtime = TrUAPIHostRuntime(HostRuntimeBridge(session.walletId, generation), localSessionSource, config).also {
            it.activateWallet(session.walletId, session.liteUsername)
        }
        BoundRuntime(runtime, session.walletId, generation)
    }

    private fun wire(bound: BoundRuntime) {
        val runtime = bound.runtime
        runtimeScope?.cancel()
        runtimeScope = CoroutineScope(scope.coroutineContext + SupervisorJob(scope.coroutineContext[Job]))
        notifyRecordsChanged()
        runtime.startStatementAllowanceRenewal()
        runtimeScope?.launch {
            runCatching { runtime.reconcileNotifications(); refreshPairedHosts(runtime) }
                .logFailure("TrUAPI startup reconciliation failed")
        }
        // Before any product execution opens, so a product never sees the
        // window where the host lists no contacts.
        runtime.setContacts(contactsBridge)
        observeContactRemovals(runtime)
        chainProvider.attach(
            onResponse = runtime::notifyChainResponse,
            onClosed = runtime::notifyChainClosed,
        )
        observeAppLifecycle()
        observeWalletAccount(runtime, bound.walletId, bound.generation)
        observeDeviceLock(runtime)
    }

    suspend fun refreshPairedHosts(selectedRuntime: TrUAPIHostRuntime? = null) {
        val runtime = selectedRuntime ?: runtime().getOrThrow()
        peerMutex.withLock {
            val generation = authGeneration.get()
            val peers = runtime.pairedHosts()
            if (generation != authGeneration.get() || sessionState.value !is AuthState.Connected) return@withLock
            pairedRecords.value = peers
            val currentKeys = peers.map { it.peerEncryption.hexKey() }.toSet()
            peerJobs.keys.toList().filterNot { it in currentKeys }.forEach { peerJobs.remove(it)?.cancel() }
            peers.forEach { record ->
                val key = record.peerEncryption.hexKey()
                if (peerJobs[key]?.isActive == true) return@forEach
                peerJobs[key] = runtimeScope!!.launch {
                    val peer = PairedSsoPeer(record.peerStatement, record.peerEncryption)
                    while (isActive) {
                        val result = runCatching { runtime.resumePairing(peer) }
                            .logFailure("TrUAPI paired session failed")
                        if (result.getOrNull() == ResponderExit.PEER_DISCONNECTED) {
                            runtime.removePairedHost(record.peerStatement, record.peerEncryption)
                            pairedRecords.value = runtime.pairedHosts()
                            break
                        }
                        delay(1_000)
                    }
                }
            }
        }
    }

    suspend fun removePairedHost(statement: ByteArray) {
        val runtime = runtime().getOrThrow()
        val record = runtime.pairedHosts().firstOrNull { it.peerStatement.contentEquals(statement) } ?: return
        peerJobs.remove(record.peerEncryption.hexKey())?.cancel()
        runtime.removePairedHost(record.peerStatement, record.peerEncryption)
        refreshPairedHosts(runtime)
    }

    suspend fun resetAccount() {
        val runtime = runtime().getOrThrow()
        workerRuntimeGeneration.incrementAndGet()
        workerSupervisor.get().stopAll()
        peerJobs.values.forEach { it.cancel() }
        runtime.resetAccount()
        runtime.shutdown()
        runtimeScope?.cancel()
        pairedRecords.value = emptyList()
        notifyRecordsChanged()
        bootMutex.withLock { boot = null }
    }

    private suspend fun buildRuntimeConfig(): HostRuntimeConfig {
        val peopleGenesis = chainRegistry.getChain(knownChains.people).genesisHash.value
        val bulletinGenesis = chainRegistry.getChain(knownChains.bulletIn).genesisHash.value
        val assetHubGenesis = chainRegistry.getChain(knownChains.assetHub).genesisHash.value
        // The core derives the wallet's reserved identities under this TLD, so it has
        // to be the one the app's own built-in accounts derive from. A wrong suffix
        // mints key material that belongs to no network, so this resolves the value
        // rather than guessing it, and fails the boot when the network cannot answer.
        // Failing releases the memoised boot so the next product load tries again.
        val networkSuffix = withTimeoutOrNull(TLD_RESOLVE_TIMEOUT_MS) {
            dotNsTldProvider.getTldRetrying()
        }?.value ?: error("dotNS TLD unresolved after ${TLD_RESOLVE_TIMEOUT_MS}ms")

        return HostRuntimeConfig(
            hostName = HOST_NAME,
            peopleChainGenesisHash = peopleGenesis,
            bulletinChainGenesisHash = bulletinGenesis,
            assetHubChainGenesisHash = assetHubGenesis,
            networkSuffix = networkSuffix,
            databaseDirectory = context.noBackupFilesDir.resolve(DATABASE_DIRECTORY).apply { mkdirs() }.absolutePath,
        )
    }

    // The core caches the contact handles it resolves; a removed or blocked
    // contact has to reach it, or their handle keeps resolving.
    private fun observeContactRemovals(runtime: TrUAPIHostRuntime) {
        runtimeScope?.launch {
            contactsBridge.contactRemovals().collect { runtime.notifyContactsChanged() }
        }
    }

    // The session is derived from the wallet's entropy, so a wallet switch
    // would otherwise leave every product signing for the previous wallet.
    private fun observeWalletAccount(runtime: TrUAPIHostRuntime, walletId: String, generation: Long) {
        runtimeScope?.launch {
            accountRepository.walletAccountFlow()
                .map { it.id.toString() }
                .distinctUntilChanged()
                .filter { it != walletId }
                .collect {
                    bootMutex.withLock {
                        if (!workerRuntimeGeneration.compareAndSet(generation, generation + 1)) return@collect
                        runtime.shutdown()
                        workerSupervisor.get().stopAll()
                        chainProvider.closeAll()
                        boot = null
                        peerJobs.clear()
                    }
                    scope.launch { runtime().logFailure("TrUAPI runtime could not follow the wallet switch") }
                }
        }
    }

    private fun observeDeviceLock(runtime: TrUAPIHostRuntime) {
        runtimeScope?.launch {
            var locked = false
            kotlinx.coroutines.flow.callbackFlow {
                val receiver = object : android.content.BroadcastReceiver() {
                    override fun onReceive(context: Context, intent: android.content.Intent) {
                        val isLocked = intent.action == android.content.Intent.ACTION_SCREEN_OFF ||
                            context.getSystemService(android.app.KeyguardManager::class.java).isDeviceLocked
                        trySend(isLocked)
                    }
                }
                val filter = android.content.IntentFilter().apply {
                    addAction(android.content.Intent.ACTION_SCREEN_OFF)
                    addAction(android.content.Intent.ACTION_SCREEN_ON)
                    addAction(android.content.Intent.ACTION_USER_PRESENT)
                }
                androidx.core.content.ContextCompat.registerReceiver(context, receiver, filter, androidx.core.content.ContextCompat.RECEIVER_NOT_EXPORTED)
                awaitClose { context.unregisterReceiver(receiver) }
            }.distinctUntilChanged().collect { isLocked ->
                runCatching {
                    if (isLocked) {
                        locked = true
                        runtime.lockWallet()
                        workerSupervisor.get().pause()
                    } else if (locked) {
                        val session = localSessionSource.resolve().getOrThrow()
                        runtime.activateWallet(session.walletId, session.liteUsername)
                        locked = false
                        workerSupervisor.get().resume()
                        runtime.reconcileNotifications()
                        refreshPairedHosts(runtime)
                    }
                }.logFailure("TrUAPI wallet lock transition failed")
            }
        }
    }

    private fun observeAppLifecycle() {
        runtimeScope?.launch {
            appLifecycleObserver.subscribe().collect { state ->
                // closeAll reports each connection back to the core, so it
                // evicts them and re-dials chainConnect on next use. Sockets do
                // not idle in background and recover on foreground.
                if (state == AppLifecycleState.BACKGROUND) chainProvider.closeAll()
            }
        }
    }

    /**
     * Serves the signing runtime only: core storage, auth state, signing-side
     * chain access and the confirmations SSO raises. Product-scoped calls have
     * no product here and fail closed, as they do on iOS.
     */
    private inner class HostRuntimeBridge(private val walletId: String, private val generation: Long) : HostBridge {
        override val secretStorage: HostSecretStorage = this@TrUAPIHostRuntimeProvider.secretStorage

        override fun onCoreLog(marker: String, detail: String) {
            Timber.tag("truapi.core").d("%s: %s", marker, detail)
        }

        override fun workerDemandChanged(productId: String, transition: WorkerTransition) {
            val demand = when (transition) {
                WorkerTransition.START -> WorkerDemand.START
                WorkerTransition.STOP -> WorkerDemand.STOP
            }
            workerSupervisor.get().onDemandChanged(ProductId.fromStoredValue(productId), demand) {
                workerRuntimeGeneration.get() == generation
            }
        }

        override suspend fun scheduleNotification(productId: String, id: UInt, request: uniffi.truapi.HostPushNotificationRequest) =
            notifications.schedule(walletId, productId, id, request)

        override suspend fun isScheduledNotificationPending(productId: String, id: UInt): Boolean =
            notifications.isPending(walletId, productId, id)

        override suspend fun cancelScheduledNotification(productId: String, id: UInt) =
            notifications.cancel(walletId, productId, id)

        override suspend fun navigateTo(url: String) {
            throw HostNavigateToException.Unknown("navigation unavailable at host level")
        }

        override suspend fun devicePermissionStatus(request: HostDevicePermissionRequest): uniffi.truapi.DevicePermissionStatus =
            hostApiInteractor.get().devicePermissionStatus(request.toCapability())

        override suspend fun devicePermission(
            product: ProductExecutionConfig,
            request: HostDevicePermissionRequest,
        ): PermissionDecision =
            hostApiInteractor.get().requestDevicePermissionDecision(ProductId.fromStoredValue(product.productId), request.toCapability()).getOrThrow().toNative()

        override suspend fun remotePermission(
            product: ProductExecutionConfig,
            request: RemotePermission,
        ): PermissionDecision =
            hostApiInteractor.get().requestRemotePermissionDecision(ProductId.fromStoredValue(product.productId), request.toDomain()).getOrThrow().toNative()

        override suspend fun confirmUserAction(review: UserConfirmationReview): Boolean =
            confirmationLauncher.decide(review, requesterFallback = HOST_REQUESTER)

        override suspend fun featureSupported(request: HostFeatureSupportedRequest): Boolean =
            when (request) {
                is HostFeatureSupportedRequest.Chain -> cachedChains.get().canDial(request.genesisHash)
            }

        override fun supportedChains(): HostChainSet = cachedChains.get().advertised

        override fun chainConnect(genesisHash: ByteArray): UInt? = chainProvider.connect(genesisHash)

        override fun chainSend(connectionId: UInt, request: String) =
            chainProvider.send(connectionId, request)

        override fun chainClose(connectionId: UInt) = chainProvider.close(connectionId)

        /**
         * Observed, not acted on. Rendering [AuthState.Pairing] as a pairing
         * sheet needs a core-driven session, and `PairingHostRuntime` is not
         * reachable from a native host yet (truapi#334, "Move SSO to the shared
         * Rust core"), so the core never reaches a state worth showing. iOS
         * stubs this the same way. Surfaced as state rather than a log line so
         * wiring the UI later is a subscription, not a rewrite.
         */
        override fun runtimeRecordsChanged() { notifyRecordsChanged() }

        override fun devicePaired(device: PairedSsoPeer) {
            runtimeScope?.launch { refreshPairedHosts() }
        }

        override fun authStateChanged(state: AuthState) {
            authGeneration.incrementAndGet()
            authState.value = state
            if (state !is AuthState.Connected) {
                pairedRecords.value = emptyList()
                peerJobs.values.forEach { it.cancel() }
                peerJobs.clear()
            }
            Timber.tag("truapi.auth").d("%s", state.marker())
        }
    }

    private companion object {
        const val HOST_NAME = "Polkadot"
        const val HOST_REQUESTER = "host"

        /**
         * Ceiling on resolving the network's dotNS TLD while booting the runtime.
         * `getTldRetrying` polls until it succeeds, so the boot needs its own bound.
         */
        const val TLD_RESOLVE_TIMEOUT_MS = 30_000L

        /**
         * Core database directory, under `noBackupFilesDir`: a durable-transaction
         * ledger restored onto another device would act on transactions that
         * already settled.
         */
        const val DATABASE_DIRECTORY = "truapi"
    }
}
