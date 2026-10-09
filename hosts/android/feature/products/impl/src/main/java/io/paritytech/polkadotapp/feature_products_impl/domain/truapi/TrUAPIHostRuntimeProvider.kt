package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import dagger.Lazy
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.HostBridge
import io.parity.truapi.HostCoreStorage
import uniffi.truapi.HostRuntimeConfig
import io.parity.truapi.HostStorage
import uniffi.truapi.ProductExecutionConfig
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.WebSocketChainProvider
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
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
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.OkHttpClient
import timber.log.Timber
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.RemotePermission
import uniffi.truapi.AuthState
import uniffi.truapi.HostChainSet
import uniffi.truapi.PermissionDecision
import uniffi.truapi.UserConfirmationReview
import uniffi.truapi.HostNavigateToException
import uniffi.truapi.HostLocalStorageReadException
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
    private val encryptedPreferences: EncryptedPreferences,
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
    private var boot: Deferred<Result<TrUAPIHostRuntime>>? = null

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
    suspend fun runtime(): Result<TrUAPIHostRuntime> {
        val pending = bootMutex.withLock {
            boot ?: scope.async { build().onSuccess(::wire) }.also { boot = it }
        }
        return pending.await().onFailure {
            bootMutex.withLock { if (boot === pending) boot = null }
        }
    }

    private suspend fun build(): Result<TrUAPIHostRuntime> = runCatching {
        val config = buildRuntimeConfig()
        cachedChains.set(chainDirectory.resolve())
        TrUAPIHostRuntime(HostRuntimeBridge(), config)
    }

    private fun wire(runtime: TrUAPIHostRuntime) {
        // Before any product execution opens, so a product never sees the
        // window where the host lists no contacts.
        runtime.setContacts(contactsBridge)
        observeContactRemovals(runtime)
        chainProvider.attach(
            onResponse = runtime::notifyChainResponse,
            onClosed = runtime::notifyChainClosed,
        )
        observeAppLifecycle()
        observeWalletAccount(runtime)
    }

    private suspend fun buildRuntimeConfig(): HostRuntimeConfig {
        val peopleGenesis = chainRegistry.getChain(knownChains.people).genesisHash.value
        val bulletinGenesis = chainRegistry.getChain(knownChains.bulletIn).genesisHash.value
        val assetHubGenesis = chainRegistry.getChain(knownChains.assetHub).genesisHash.value
        // Booting without a session is the pre-session behaviour: products load
        // and every signing call fails. Worth degrading to rather than refusing
        // every product outright.
        val localSession = localSessionSource.resolve()
            .logFailure("TrUAPI local session unavailable; booting the host runtime without one")
            .getOrNull()
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
            hostVersion = hostVersion(),
            peopleChainGenesisHash = peopleGenesis,
            bulletinChainGenesisHash = bulletinGenesis,
            assetHubChainGenesisHash = assetHubGenesis,
            networkSuffix = networkSuffix,
            localSessionSecret = localSession?.secret,
            localSessionLiteUsername = localSession?.liteUsername,
            databaseDirectory = context.noBackupFilesDir.resolve(DATABASE_DIRECTORY).apply { mkdirs() }.absolutePath,
        )
    }

    private fun hostVersion(): String {
        val packageInfo = context.packageManager.getPackageInfo(context.packageName, 0)
        return "${packageInfo.versionName.orEmpty()} (${packageInfo.longVersionCode})"
    }

    // The core caches the contact handles it resolves; a removed or blocked
    // contact has to reach it, or their handle keeps resolving.
    private fun observeContactRemovals(runtime: TrUAPIHostRuntime) {
        scope.launch {
            contactsBridge.contactRemovals().collect { runtime.notifyContactsChanged() }
        }
    }

    // The session is derived from the wallet's entropy, so a wallet switch
    // would otherwise leave every product signing for the previous wallet.
    private fun observeWalletAccount(runtime: TrUAPIHostRuntime) {
        scope.launch {
            accountRepository.walletAccountFlow()
                .map { it.id }
                .distinctUntilChanged()
                .drop(1)
                .collect {
                    localSessionSource.resolve()
                        .mapCatching { session -> runtime.activateLocalSession(session.secret, session.liteUsername) }
                        .logFailure("TrUAPI local session could not follow the wallet switch")
                }
        }
    }

    private fun observeAppLifecycle() {
        scope.launch {
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
    private inner class HostRuntimeBridge : HostBridge {
        override val storage: HostStorage = HostLevelStorage

        override val coreStorage: HostCoreStorage = EncryptedHostCoreStorage(encryptedPreferences)

        override fun onCoreLog(marker: String, detail: String) {
            Timber.tag("truapi.core").d("%s: %s", marker, detail)
        }

        // Runtime-wide demand from the core's worker ledger; the supervisor marshals it off this thread.
        override fun workerDemandChanged(productId: String, transition: WorkerTransition) {
            val demand = when (transition) {
                WorkerTransition.START -> WorkerDemand.START
                WorkerTransition.STOP -> WorkerDemand.STOP
            }
            workerSupervisor.get().onDemandChanged(ProductId.fromStoredValue(productId), demand)
        }

        override suspend fun navigateTo(url: String) {
            throw HostNavigateToException.Unknown("navigation unavailable at host level")
        }

        override suspend fun devicePermission(
            product: ProductExecutionConfig,
            request: HostDevicePermissionRequest,
        ): PermissionDecision =
            PermissionDecision.DENY

        override suspend fun remotePermission(
            product: ProductExecutionConfig,
            request: RemotePermission,
        ): PermissionDecision =
            PermissionDecision.DENY

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
        override fun authStateChanged(state: AuthState) {
            authState.value = state
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

private object HostLevelStorage : HostStorage {
    override suspend fun read(key: String): ByteArray? = null

    override suspend fun write(key: String, value: ByteArray) = throw noProductScope()

    override suspend fun clear(key: String) = throw noProductScope()

    private fun noProductScope() =
        HostLocalStorageReadException.Unknown("no product scope at host level")
}
