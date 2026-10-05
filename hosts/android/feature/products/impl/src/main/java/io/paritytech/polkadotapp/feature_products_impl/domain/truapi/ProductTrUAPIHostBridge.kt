package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import androidx.core.net.toUri
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.parity.truapi.HostBridge
import io.parity.truapi.HostCoreStorage
import io.parity.truapi.HostStorage
import io.parity.truapi.LocalhostBridgeBootstrap
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.ProductExecutionKind
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.parity.truapi.WebSocketChainProvider
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toUri
import io.paritytech.polkadotapp.feature_products_impl.di.TrUAPIChainHttpClient
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ProductTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ThemeVariant
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.NotificationId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.RemotePermissionRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketCardStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import timber.log.Timber
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostPushNotificationRequest
import uniffi.truapi.HostThemeSubscribeItem
import uniffi.truapi.RemotePermission
import uniffi.truapi.ThemeName
import uniffi.truapi.AuthState
import uniffi.truapi.HostChainSet
import uniffi.truapi.UserConfirmationReview
import uniffi.truapi.HostNavigateToException
import uniffi.truapi.HostRejection
import java.util.concurrent.atomic.AtomicReference
import kotlin.time.Instant
import uniffi.truapi.ThemeVariant as NativeThemeVariant
import uniffi.truapi.PermissionDecision as TrUAPIPermissionDecision

/**
 * Native platform callbacks ([io.parity.truapi.HostBridge]) for one product
 * WebView, opened as a [TrUAPIProductExecution] on the shared host runtime.
 * Widget and custom-message rendering is intentionally unsupported, SPA
 * products only.
 *
 * Threading: the core invokes every callback off the UI thread. The
 * prompt-driven ones (navigateTo, devicePermission, remotePermission,
 * featureSupported, confirmUserAction) are suspend and awaited by the core, so
 * the handlers below call the suspending [HostApiInteractor] APIs directly and
 * may stay pending until the user decides. The rest run inline on the core's
 * dispatcher thread and must return promptly.
 */
class ProductTrUAPIHostBridge @AssistedInject constructor(
    private val hostApiInteractor: HostApiInteractor,
    @param:TrUAPIChainHttpClient private val chainHttpClient: OkHttpClient,
    private val encryptedPreferences: EncryptedPreferences,
    private val confirmationLauncher: TrUAPIConfirmationLauncher,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val dotNsTldProvider: DotNsTldProvider,
    private val pocketCardStore: PocketCardStore,
    @Assisted private val scope: CoroutineScope,
) {
    @AssistedFactory
    interface Factory {
        fun create(scope: CoroutineScope): ProductTrUAPIHostBridge
    }

    // The app's own theme is not known until subscribeTheme() emits; until then
    // the core gets the host-default answer the shell documents.
    private val cachedTheme = AtomicReference(
        HostThemeSubscribeItem(ThemeName.Default, NativeThemeVariant.DARK),
    )

    // Resolved on attach(): the core asks for chains on its dispatcher thread,
    // where a suspending registry lookup is not allowed.
    private val cachedChains = AtomicReference(EMPTY_CHAINS)

    private val chainProvider = WebSocketChainProvider(
        // Pure lookup against the snapshot resolved on attach(). The core calls
        // this inline on its dispatcher thread, where blocking on the registry
        // would stall unrelated TrUAPI traffic.
        resolver = { genesisHash -> cachedChains.get().endpoints[genesisHash.hexKey()].orEmpty() },
        client = chainHttpClient,
        onLog = { Timber.tag("truapi.chain").d("%s", it) },
    )

    private var execution: TrUAPIProductExecution? = null
    private var pocketBridge: ProductPocketHostBridge? = null

    init {
        // Tear the execution down with the owning scope: otherwise a closed
        // product leaves a live connection, its loopback WS listener, and chain
        // sockets behind.
        scope.coroutineContext.job.invokeOnCompletion { stop() }
    }

    /**
     * Built in [attach], where the product id is known, so the storage
     * namespace and every product-scoped call are fixed at construction
     * instead of resolved from mutable state the callbacks trust.
     */
    private fun buildBridge(
        callingProductId: ProductId,
        navigation: NavigationPolicy,
    ) = object : HostBridge {
        override val storage: HostStorage =
            EncryptedHostStorage(encryptedPreferences, callingProductId.value)

        override val coreStorage: HostCoreStorage = EncryptedHostCoreStorage(encryptedPreferences)

        override fun onCoreLog(marker: String, detail: String) {
            Timber.tag("truapi.core").d("%s: %s", marker, detail)
        }

        /**
         * Classified and dispatched by the same rule as the native `navigateTo`
         * handler, so a link cannot resolve differently depending on the
         * runtime toggle. Opening every URL externally would push in-app dotNS
         * navigation into the system browser, where those names do not resolve.
         */
        override suspend fun navigateTo(url: String) {
            val destination = url.toUri()
            val tld = dotNsTldProvider.getTld().getOrElse {
                throw HostNavigateToException.Unknown(it.message.orEmpty())
            }
            val type = DotNsUtils.classifyNavigation(callingProductId.toUri(), destination, tld)
            withContext(Dispatchers.Main) {
                runCatching { navigation.handleNavigation(type, destination) }
                    .getOrElse {
                        throw HostNavigateToException.Unknown(it.message.orEmpty())
                    }
            }
        }

        override suspend fun pushNotification(request: HostPushNotificationRequest): UInt =
            hostApiInteractor
                .publishNotificationAuthorized(
                    callingProductId = callingProductId,
                    text = request.text,
                    deeplink = request.deeplink,
                    // Wire carries Unix millis UTC; null fires immediately.
                    scheduledAt = request.scheduledAt?.let { Instant.fromEpochMilliseconds(it.toLong()) },
                )
                .map { it.value.toUInt() }
                .getOrElse { throw HostRejection.Rejected(it.message.orEmpty()) }

        override fun cancelNotification(id: UInt) {
            // Runs inline on the dispatcher thread, so it must not block on the
            // suspending scheduler; hand it to the session scope instead.
            scope.launch {
                hostApiInteractor
                    .cancelNotificationAuthorized(callingProductId, NotificationId(id.toInt()))
                    .logFailure("truapi.cancel_notification: $id")
            }
        }

        override suspend fun lookupPreimage(key: ByteArray): ByteArray? =
            hostApiInteractor.lookupPreimage(key).getOrNull()

        /**
         * Session state belongs to the shared runtime and is observed there
         * ([TrUAPIHostRuntimeProvider.sessionState]); an execution only logs it.
         */
        override fun authStateChanged(state: AuthState) {
            Timber.tag("truapi.auth").d("%s: %s", callingProductId.value, state.marker())
        }

        override suspend fun confirmUserAction(review: UserConfirmationReview): Boolean =
            confirmationLauncher.decide(review, requesterFallback = callingProductId.value)

        override suspend fun devicePermission(
            product: ProductExecutionConfig,
            request: HostDevicePermissionRequest,
        ): TrUAPIPermissionDecision =
            hostApiInteractor
                .requestDevicePermissionDecision(callingProductId, request.toCapability())
                .getOrElse { throw it }
                .toNative()

        override suspend fun remotePermission(
            product: ProductExecutionConfig,
            request: RemotePermission,
        ): TrUAPIPermissionDecision =
            hostApiInteractor
                .requestRemotePermissionDecision(callingProductId, request.toDomain())
                .getOrElse { throw it }
                .toNative()

        /**
         * Answered from the same snapshot [chainConnect] dials rather than the
         * registry: a registry-only yes advertises chains that have no
         * endpoints, which `chainConnect` then refuses, so the core would open
         * connections the host cannot serve.
         */
        override suspend fun featureSupported(request: HostFeatureSupportedRequest): Boolean =
            when (request) {
                is HostFeatureSupportedRequest.Chain -> cachedChains.get().canDial(request.genesisHash)
            }

        override fun currentTheme(): HostThemeSubscribeItem = cachedTheme.get()

        override fun supportedChains(): HostChainSet = cachedChains.get().advertised

        override fun chainConnect(genesisHash: ByteArray): UInt? = chainProvider.connect(genesisHash)

        override fun chainSend(connectionId: UInt, request: String) =
            chainProvider.send(connectionId, request)

        override fun chainClose(connectionId: UInt) = chainProvider.close(connectionId)
    }

    /**
     * Opens the product's execution on the shared runtime and hands the caller
     * the bootstrap script. It must be injected before the product page loads
     * or the client never connects.
     *
     * A second call opens nothing and answers with the execution already
     * running: opening another would leak the first, along with its loopback
     * listener and chain sockets.
     */
    suspend fun attach(
        runtime: TrUAPIHostRuntime,
        productId: ProductId,
        chains: TrUAPIChains,
        navigationPolicy: NavigationPolicy,
        kind: ProductExecutionKind,
        onReadyToInject: suspend (bootstrap: String) -> Unit,
    ): Result<TrUAPIProductExecution> {
        execution?.let {
            Timber.w("truapi.attach: already attached to %s, ignoring", productId.value)
            return Result.success(it)
        }
        // Opening the execution is inside the Result too: it reaches the core and can be refused,
        // and the callers launch this into scopes that have no handler for a throw.
        return runCatching {
            cachedChains.set(chains)
            val pocket = ProductPocketHostBridge(productId, pocketCardStore, scope)
            val opened = runtime.openProductExecution(
                bridge = buildBridge(productId, navigationPolicy),
                configuration = ProductExecutionConfig(productId.value, kind),
                pocket = pocket,
            )
            execution = opened
            pocketBridge = pocket
            // Anything failing past this point leaves a live execution behind, and
            // `execution != null` would then block every re-attach; tear it down.
            pocket.start(opened::notifyPocketCardsChanged)
            chainProvider.attach(
                onResponse = opened::notifyChainResponse,
                onClosed = opened::notifyChainClosed,
            )
            val endpoint = opened.startWsBridge()
            observeAppTheme()
            observeAppLifecycle()
            onReadyToInject(LocalhostBridgeBootstrap.script(endpoint.port, endpoint.token))
            opened
        }.onFailure { stop() }
    }

    private fun observeAppLifecycle() {
        scope.launch {
            appLifecycleObserver.subscribe().collect { state ->
                // closeAll reports each connection back to the core, so it
                // evicts them and re-dials chainConnect on next use. Sockets do
                // not idle in background and recover on foreground. Teardown
                // takes the other path: stop() detaches first, so the core is
                // not notified about an execution that is going away anyway.
                if (state == AppLifecycleState.BACKGROUND) chainProvider.closeAll()
            }
        }
    }

    private fun observeAppTheme() {
        scope.launch {
            hostApiInteractor.subscribeTheme().collect { setTheme(it.toNativeTheme()) }
        }
    }

    private fun setTheme(theme: HostThemeSubscribeItem) {
        cachedTheme.set(theme)
        execution?.notifyThemeChanged(theme)
    }

    /**
     * Tears down the execution and its chain connections; the shared runtime
     * stays up for other products. Idempotent. Detaches the provider before
     * closing sockets so a closing socket cannot notify an execution that is
     * being disposed.
     */
    fun stop() {
        val opened = execution ?: return
        execution = null
        // Before the execution closes: a card change arriving afterwards would republish through a
        // handle that no longer exists.
        pocketBridge?.stop()
        pocketBridge = null
        chainProvider.detach()
        chainProvider.closeAll()
        opened.stopWsBridge()
        // Shuts the connection down and releases the native handle. Without it
        // the execution outlives the product tab even though its bridge and
        // sockets are gone.
        opened.close()
    }
}

/**
 * Confirm-only: the core owns the key and signs after approval, so this
 * answers yes/no and never produces a signature. A review the app cannot
 * describe fails closed, but that is a mapping bug rather than the normal
 * path. [requesterFallback] names the requester for the one review that does
 * not carry a product id itself.
 */
internal suspend fun TrUAPIConfirmationLauncher.decide(
    review: UserConfirmationReview,
    requesterFallback: String,
): Boolean {
    val confirmation = runCatching { review.toConfirmation(requesterFallback) }
        .getOrElse {
            Timber.w(it, "truapi.confirm: could not describe review, rejecting")
            return false
        }

    return awaitDecision(confirmation)
}

// Reports the theme name the native host's `themeSubscribe` already sends, so a
// product reads the same theme on either runtime.
private fun ProductTheme.toNativeTheme(): HostThemeSubscribeItem = HostThemeSubscribeItem(
    name = ThemeName.Custom(name),
    variant = when (variant) {
        ThemeVariant.Light -> NativeThemeVariant.LIGHT
        ThemeVariant.Dark -> NativeThemeVariant.DARK
    },
)

private fun HostDevicePermissionRequest.toCapability(): DeviceCapabilityType = when (this) {
    HostDevicePermissionRequest.NOTIFICATIONS -> DeviceCapabilityType.Notifications
    HostDevicePermissionRequest.CAMERA -> DeviceCapabilityType.Camera
    HostDevicePermissionRequest.MICROPHONE -> DeviceCapabilityType.Microphone
    HostDevicePermissionRequest.BLUETOOTH -> DeviceCapabilityType.Bluetooth
    HostDevicePermissionRequest.NFC -> DeviceCapabilityType.NFC
    HostDevicePermissionRequest.LOCATION -> DeviceCapabilityType.Location
    HostDevicePermissionRequest.CLIPBOARD -> DeviceCapabilityType.Clipboard
    HostDevicePermissionRequest.OPEN_URL -> DeviceCapabilityType.OpenUrl
    HostDevicePermissionRequest.BIOMETRICS -> DeviceCapabilityType.Biometrics
}

private fun RemotePermission.toDomain(): RemotePermissionRequest = when (this) {
    is RemotePermission.Remote -> RemotePermissionRequest.Remote(domains)
    RemotePermission.WebRtc -> RemotePermissionRequest.WebRtc
    RemotePermission.ChainSubmit -> RemotePermissionRequest.ChainSubmit
    RemotePermission.PreimageSubmit -> RemotePermissionRequest.PreimageSubmit
    RemotePermission.StatementSubmit -> RemotePermissionRequest.StatementSubmit
}

private fun PermissionDecision.toNative(): TrUAPIPermissionDecision = when (this) {
    PermissionDecision.AllowOnce -> TrUAPIPermissionDecision.ALLOW_ONCE
    PermissionDecision.AllowAlways -> TrUAPIPermissionDecision.ALLOW_ALWAYS
    PermissionDecision.Deny -> TrUAPIPermissionDecision.DENY
}

internal fun AuthState.marker(): String = when (this) {
    is AuthState.Disconnected -> "disconnected"
    is AuthState.Pairing -> "pairing"
    is AuthState.Connected -> "connected"
    is AuthState.LoginFailed -> "login_failed: $reason"
    is AuthState.Authenticating -> "authenticating"
}
