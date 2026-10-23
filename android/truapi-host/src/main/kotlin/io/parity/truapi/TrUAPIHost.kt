// TrUAPIHost - Android host adapter.
//
// The Rust core (compiled to `libtruapi.so` and surfaced via UniFFI in
// `src/main/kotlin/generated/uniffi/truapi/truapi.kt`) owns the
// wire protocol, request routing, subscription lifecycle, and platform trait
// dispatch.
//
// This file exposes:
//
//   * `HostBridge` - the Kotlin-friendly callback interface the embedding app
//     implements. It splits device and remote permissions, mirroring the
//     `Permissions` platform trait in the Rust core.
//   * `HostStorage` / `HostCoreStorage` - the product-scoped and core-owned
//     key-value backends the host persists.
//   * `TrUAPIHostRuntime` / `TrUAPIProductExecution` - process-owned host state
//     and independently scoped product connections.
//   * `LocalhostBridgeBootstrap` - private endpoint configuration consumed by
//     the shared browser container before product scripts run.
//
// Products running inside a `WebView` connect to the Rust core via the
// localhost WebSocket bridge. Start it with `execution.startWsBridge()` and load
// the product page after injecting `LocalhostBridgeBootstrap.script(...)` and
// `ContainerScriptBundle.load(...)` at document start. The container publishes
// `window.__HOST_API_CLIENT__` and a compatibility MessagePort for older SDKs.

package io.parity.truapi

import java.util.Locale
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.conflate
import uniffi.truapi.ChatResource
import uniffi.truapi.ChatRoomCall
import uniffi.truapi.ChatRoomEvent
import uniffi.truapi.ChatRoomContact
import uniffi.truapi.ChatRoomFeatures
import uniffi.truapi.ChatWakeOutcome
import uniffi.truapi.ChatWakeReason
import uniffi.truapi.NativeChatStageRejection
import uniffi.truapi.WorkerKeepAlive
import uniffi.truapi.ProductChatOpenLinkRequest
import uniffi.truapi.ProductChatOpenLinkItem
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomConfig
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatUserAction
import uniffi.truapi.HostChatActionSubscribeItem
import uniffi.truapi.NativeChatFeedRejection
import uniffi.truapi.NativeChatQuery
import uniffi.truapi.NativeChatQueryAnswer
import uniffi.truapi.NativeChatQueryObserver
import uniffi.truapi.NativeChatQueryRefusal
import uniffi.truapi.ProductChatEditHistoryQueryItem
import uniffi.truapi.ProductChatEditHistoryQueryRequest
import uniffi.truapi.ProductChatMessagesQueryItem
import uniffi.truapi.ProductChatMessagesQueryRequest
import uniffi.truapi.ProductChatQueryError
import uniffi.truapi.ProductChatSearchQueryItem
import uniffi.truapi.ProductChatSearchQueryRequest
import uniffi.truapi.ProductChatUserActionError
import uniffi.truapi.ProductChatUserActionItem
import uniffi.truapi.ProductChatUserActionRequest
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostLocaleSubscribeItem
import uniffi.truapi.PocketCard
import uniffi.truapi.HostPushNotificationRequest
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RemotePermission
import uniffi.truapi.RemotePermissionRequest
import uniffi.truapi.RendererNode
import uniffi.truapi.HostThemeSubscribeItem
import uniffi.truapi.ThemeName
import uniffi.truapi.ThemeVariant
import uniffi.truapi.AuthState
import uniffi.truapi.HostChainSet
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.PermissionDecision
import uniffi.truapi.UserConfirmationReview
import uniffi.truapi.DbStatus
import uniffi.truapi.HostCallbacks
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.NativeCallMediaCallbacks
import uniffi.truapi.NativeChatCallbacks
import uniffi.truapi.NativeCoreDatabaseException
import uniffi.truapi.RecoveryException
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.NativeGameCallbacks
import uniffi.truapi.NativePocketCallbacks
import uniffi.truapi.NativePocketRemoval
import uniffi.truapi.NativeRendererObserver
import uniffi.truapi.DevicePermissionStatus
import uniffi.truapi.ExpandedCardFaceOutcome
import uniffi.truapi.NativeProductExecution
import uniffi.truapi.NativeTrUApiHostRuntime
import uniffi.truapi.NativeAnnouncedPairing
import uniffi.truapi.PairedSsoPeer
import uniffi.truapi.ResponderExit
import uniffi.truapi.ProductRuntimeException
import uniffi.truapi.HostNavigateToException
import uniffi.truapi.HostRejection
import uniffi.truapi.HostLocalStorageReadException
import uniffi.truapi.localhostBridgeBootstrapScript
import uniffi.truapi.NativeRuntimeConfigException
import uniffi.truapi.StatementRenewalTarget
import uniffi.truapi.TrackedStatementRenewalTarget
import uniffi.truapi.StatementRenewalReport
import uniffi.truapi.WorkerTransition
import uniffi.truapi.WsBridgeEndpoint
import uniffi.truapi.WsBridgeStartException
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.HostContactLookup
import uniffi.truapi.HostContactMatches
import uniffi.truapi.HostContactPick
import uniffi.truapi.NativeContactsCallbacks
import uniffi.truapi.HostPaymentBalanceSubscribeException
import uniffi.truapi.HostPaymentException
import uniffi.truapi.HostPaymentRequest
import uniffi.truapi.HostPaymentStatusSubscribeItem
import uniffi.truapi.HostPaymentTopUpException
import uniffi.truapi.HostPaymentTopUpRequest
import uniffi.truapi.HostPaymentTopUpStatusSubscribeItem
import uniffi.truapi.HostScan
import uniffi.truapi.HostScannerScanRequest
import uniffi.truapi.NativeBalanceCallbacks
import uniffi.truapi.NativePaymentCallbacks
import uniffi.truapi.NativeScannerCallbacks
import uniffi.truapi.NativeTopUpCallbacks
import uniffi.truapi.NativePersonChatCallbacks
import uniffi.truapi.HostChatContact
import uniffi.truapi.HostChatPaymentReceipt
import uniffi.truapi.HostHistoryExportChunk
import uniffi.truapi.HostPersonChatPaymentReceivedRequest
import uniffi.truapi.HostPairedDevice
import uniffi.truapi.NativePushCallbacks
import uniffi.truapi.HostChatPushRoute
import uniffi.truapi.HostPushDelivery
import uniffi.truapi.HostPushTokens
import uniffi.truapi.NativeHopDownloadProgress
import uniffi.truapi.NativeHopFile
import uniffi.truapi.NativeHopUploadProgress
import uniffi.truapi.ProductExecutionKind

/** Package metadata. */
object TrUAPIHost {
    const val VERSION = "0.1.0"
}

/**
 * Product-scoped key-value storage the host provides to the Rust core. Throws
 * [HostLocalStorageReadException] to signal quota exhaustion or unknown failure; the
 * variants are the v0.1 `HostLocalStorageReadError` wire shape.
 */
interface HostStorage {
    @Throws(HostLocalStorageReadException::class)
    suspend fun read(key: String): ByteArray?

    @Throws(HostLocalStorageReadException::class)
    suspend fun write(key: String, value: ByteArray)

    @Throws(HostLocalStorageReadException::class)
    suspend fun clear(key: String)
}

/**
 * Core-owned key-value storage the host backs with its own persistence. The
 * core writes auth session, pairing identity, and persisted permission
 * decisions here; [key] is a SCALE-encoded `CoreStorageKey`. Throws
 * [HostRejection] on failure.
 */
interface HostCoreStorage {
    @Throws(HostRejection::class)
    suspend fun read(key: ByteArray): ByteArray?

    @Throws(HostRejection::class)
    suspend fun write(key: ByteArray, value: ByteArray)

    @Throws(HostRejection::class)
    suspend fun clear(key: ByteArray)
}

/** Ids handed out by the default [HostBridge.beginOperation], distinct for the life of the process. */
private val defaultOperationIds = AtomicInteger(0)

/**
 * Host-side callback bundle that the Rust core invokes for capabilities the
 * native shell owns. The interface mirrors the underlying UniFFI surface but
 * keeps the permission split explicit:
 *
 *   * [devicePermission] handles camera / mic / push prompts and similar
 *     OS-scoped grants.
 *   * [remotePermission] handles per-product capabilities requested by the
 *     application running inside the WebView.
 *
 * Embedders render the typed request values in their own UI, then report the
 * user's decision as a `PermissionDecision`.
 *
 * The Rust core invokes callbacks on its shared background bridge executor.
 * Suspend callbacks while waiting for a decision; blocking their thread stalls
 * other TrUAPI traffic. Synchronous callbacks must return promptly. Run UI work
 * on the main thread, for example with `withContext(Dispatchers.Main) { ... }`.
 */
interface HostBridge {
    /** Lifecycle logger. Marker is a stable slug, detail is free-form. */
    fun onCoreLog(marker: String, detail: String) {}

    /**
     * Open a URL in the system browser, suspending for any approval on the main thread.
     */
    @Throws(HostNavigateToException::class)
    suspend fun navigateTo(url: String)

    /**
     * Deliver a push notification and return the host-assigned notification
     * id. Run any UI work on the main thread.
     */
    @Throws(HostRejection::class)
    suspend fun pushNotification(request: HostPushNotificationRequest): UInt = 0u

    /** Cancel a previously scheduled notification id. */
    @Throws(HostRejection::class)
    fun cancelNotification(id: UInt) {}

    /**
     * Prompt for a device-level permission [product] requested on the main
     * thread, suspending until the user decides. Preserve whether approval
     * applies once or always.
     */
    @Throws(HostRejection::class)
    suspend fun devicePermission(
        product: ProductExecutionConfig,
        request: HostDevicePermissionRequest,
    ): PermissionDecision

    /**
     * Report the OS status of a device capability without prompting. Answer from
     * `ContextCompat.checkSelfPermission` and friends.
     *
     * The core calls this before every device-permission request and status
     * read, so it must not show UI. A capability with no OS gate on Android
     * answers [DevicePermissionStatus.NOT_APPLICABLE], which leaves the stored
     * product decision governing.
     *
     * Note that Android auto-revokes runtime permissions for unused apps, which
     * surfaces here as [DevicePermissionStatus.NOT_DETERMINED]. The core does not
     * treat that as a refusal, so re-requesting is the host's call.
     *
     * Defaults to [DevicePermissionStatus.NOT_APPLICABLE], so an app that does
     * not implement it keeps today's behaviour.
     */
    suspend fun devicePermissionStatus(
        request: HostDevicePermissionRequest,
    ): DevicePermissionStatus = DevicePermissionStatus.NOT_APPLICABLE

    /**
     * Show or hide the face above this execution's expanded card. Answers
     * [ExpandedCardFaceOutcome.NOT_PRESENTED] when the product is not under its
     * card and [ExpandedCardFaceOutcome.USER_MOVING] while the user drags it,
     * and returns without waiting for the animation.
     *
     * Defaults to [ExpandedCardFaceOutcome.UNSUPPORTED], so an app without cards
     * says so instead of pretending it moved one.
     */
    suspend fun setExpandedCardFaceShown(shown: Boolean): ExpandedCardFaceOutcome =
        ExpandedCardFaceOutcome.UNSUPPORTED

    /**
     * Prompt for a remote permission bundle [product] requested on the main
     * thread, suspending until the user decides.
     */
    @Throws(HostRejection::class)
    suspend fun remotePermission(
        product: ProductExecutionConfig,
        request: RemotePermission,
    ): PermissionDecision

    /**
     * Observe an auth state change, in transition order: render
     * [AuthState.Pairing] as the pairing QR UI, connected/disconnected as the
     * account badge, and login-failed as a retryable error, unless its kind is
     * [LoginFailureKind.NoFreeAllowanceSlots], which is unlikely to succeed
     * before the period rolls over, so retry should not be the primary action.
     * A pairing host's session activation reports its
     * outcome even when it is the default disconnected, so a host that awaits
     * activation before routing never has to read silence as "signed out";
     * every other emission, and every emission on a host role that has no
     * session activation, happens only when the state actually changes.
     * Invoked on the dispatcher thread; marshal the state to the main thread
     * and return promptly.
     */
    fun authStateChanged(state: AuthState) {}

    /** Open a JSON-RPC chain connection and return a host-assigned id, or null if unsupported. */
    @Throws(HostRejection::class)
    fun chainConnect(genesisHash: ByteArray): UInt? = null

    /**
     * Send one JSON-RPC request on a native chain connection. Enqueue it and
     * return without waiting on the network, so requests go out in call order.
     */
    @Throws(HostRejection::class)
    fun chainSend(connectionId: UInt, request: String) {}

    /** Close a native chain connection. Enqueue the close and return without waiting on the network. */
    @Throws(HostRejection::class)
    fun chainClose(connectionId: UInt) {}

    /**
     * Confirm one user-reviewed core action; the review variant picks the
     * prompt (sign payload, sign raw, create transaction, resource allocation,
     * or preimage submit). Present it on the main thread, suspending until the
     * user decides.
     */
    @Throws(HostRejection::class)
    suspend fun confirmUserAction(review: UserConfirmationReview): Boolean = false

    /** Preserve the selected lifetime for identity and account access consent. */
    @Throws(HostRejection::class)
    suspend fun confirmPermission(review: UserConfirmationReview): PermissionDecision =
        if (confirmUserAction(review)) PermissionDecision.ALLOW_ALWAYS else PermissionDecision.DENY

    /** Return the current preimage value for [key], or null for a miss. */
    @Throws(HostRejection::class)
    suspend fun lookupPreimage(key: ByteArray): ByteArray? = null

    /** Return the current host theme. Hosts with no named themes report [ThemeName.Default]. */
    @Throws(HostRejection::class)
    fun currentTheme(): HostThemeSubscribeItem =
        HostThemeSubscribeItem(ThemeName.Default, ThemeVariant.DARK)

    /**
     * Return the language this host presents its interface in, as a BCP 47 tag.
     * Hosts with no in-app language picker report the system language.
     */
    @Throws(HostRejection::class)
    fun currentLocale(): HostLocaleSubscribeItem =
        HostLocaleSubscribeItem(Locale.getDefault().toLanguageTag())

    /**
     * Answer a feature-support query. Invoked on the dispatcher thread; must
     * return promptly.
     */
    @Throws(HostRejection::class)
    suspend fun featureSupported(request: HostFeatureSupportedRequest): Boolean

    /**
     * Enumerate the chains this host serves: its environment plus one entry
     * per chain role.
     */
    @Throws(HostRejection::class)
    fun supportedChains(): HostChainSet = HostChainSet(network = "", chains = emptyList())

    /**
     * Observe demand on a product's worker crossing zero. `Start` means run
     * the worker now, `Stop` that nothing wants it any more. Every transition
     * arrives here in ledger order, the ones the app asks for by taking a
     * reference of its own included.
     *
     * Demand is runtime-wide, so the core invokes this only on the bridge
     * [TrUAPIHostRuntime] was built with, never on the per-execution bridge
     * passed to [TrUAPIHostRuntime.openProductExecution]. Can arrive on any
     * thread, including synchronously on the calling thread during
     * `acquireWorker`/`releaseWorker`, often the main thread and
     * re-entrantly: marshal the work off rather than blocking on another
     * thread from inside it.
     */
    fun workerDemandChanged(productId: String, transition: WorkerTransition) {}

    /**
     * Begin a pending operation. [label] is a log/UI hint, empty when the
     * product gave none. Leave unimplemented to opt out of worker keep-alive;
     * override to run background work past the product's surface.
     *
     * The default id is still distinct per call, because an operation id names
     * one operation: a host overriding only [endOperation], and the core's own
     * demand accounting, both end the wrong ones when every operation shares an
     * id.
     */
    @Throws(HostRejection::class)
    suspend fun beginOperation(productId: String, label: String): UInt =
        defaultOperationIds.incrementAndGet().toUInt()

    /** End a pending operation. Idempotent, so a retry after an ambiguous failure is safe. */
    @Throws(HostRejection::class)
    suspend fun endOperation(productId: String, id: UInt) {}

    /**
     * A device finished pairing with this signing host.
     *
     * The core has no chat of its own, so announcing the new device to the
     * user's existing contacts is the host's to do. At least once per
     * pairing, and the host keeps its own record of which devices it has
     * already seen: a resumed pairing reports nothing and the core has no
     * list to replay. Arrives on the thread answering the handshake, while
     * the pairing call is still running: marshal the work off rather than
     * announcing it inline.
     */
    fun devicePaired(device: PairedSsoPeer) {}

    /**
     * Whether the core has durable transactions still awaiting a verdict.
     * While [pending] is true, keep a background task running that awaits
     * [TrUAPIHostRuntime.runDurableRecovery]; false means nothing needs it.
     * Reported on change, starting with the state the runtime finds at
     * launch. Runtime-wide, like [workerDemandChanged], and arrives on a core
     * thread: marshal the work off rather than blocking.
     */
    fun durableWorkChanged(pending: Boolean) {}

    /** Product-scoped key-value storage for the Rust core. */
    val storage: HostStorage

    /** Core-owned key-value storage for auth session / pairing identity / permission decisions. */
    val coreStorage: HostCoreStorage
}

/**
 * Native Chat storage and UI surface. Implement and pass to
 * [TrUAPIHostRuntime.openProductExecution] when the host supports the Chat
 * modality; hosts without it pass nothing.
 *
 * Threading: these run on the process-wide dispatch pool shared by every
 * product execution, so implementations must be safe to enter concurrently.
 * Marshal UI work to the main thread.
 */
interface ChatHostBridge {
    /**
     * Create or resolve a native product Chat room. The core has bounded and
     * normalized these arguments and screened the icon scheme; escaping them
     * for the surface that renders them is still the host's job.
     */
    @Throws(HostRejection::class)
    suspend fun createRoom(roomId: String, name: String, icon: String): ChatRoomRegistrationStatus

    /**
     * Register or resolve a native product Chat bot. The core has bounded and
     * normalized these arguments and screened the icon scheme; escaping them
     * for the surface that renders them is still the host's job.
     */
    @Throws(HostRejection::class)
    suspend fun registerBot(botId: String, name: String, icon: String): ChatBotRegistrationStatus

    /**
     * Persist a product-authored message in native Chat storage. Throw for a
     * content variant this host cannot render.
     *
     * The core has bounded and screened every field, but a body passes through
     * byte-for-byte and `ChatFile.sizeBytes` is an unverified product
     * assertion, so escaping and sizing remain the host's job.
     *
     * The returned id is what `ActionTrigger.messageId` carries back, so it
     * must name this message for as long as the host stores it. An id arriving
     * in a `Reaction` or `ReactionRemoved` is product-chosen and untrusted: it
     * may name a message in another room, or none at all.
     *
     * `alt` is the product's one-line description of the message, already
     * trimmed and screened, for places that list it without drawing it.
     */
    @Throws(HostRejection::class)
    suspend fun postMessage(roomId: String, content: ChatMessageContent, alt: String?): String

    /**
     * Set what a product's native Chat room shows below its messages, and keep
     * it until the product sets another. A room without a footer shows the
     * text input.
     */
    @Throws(HostRejection::class)
    suspend fun setRoomFooter(roomId: String, footer: ChatRoomFooter)

    /** Return the current product-scoped native Chat rooms. */
    @Throws(HostRejection::class)
    suspend fun listRooms(): List<ChatRoom>

    // The members below have defaults for a host that does not serve the Chat
    // Extension v2 requests yet. The product receives a rejection for each
    // request, except for metadata, which the host ignores and keeps the name
    // and icon of `createRoom`, and for a configuration that keeps the
    // behaviour the host serves anyway.

    /**
     * Return the configuration the product last declared for a room, or
     * [HOST_DEFAULT_CHAT_ROOM_CONFIG] when it declared none. The core reads it
     * before each feed update, posted message and query of the room.
     */
    @Throws(HostRejection::class)
    suspend fun roomConfig(roomId: String): ChatRoomConfig = HOST_DEFAULT_CHAT_ROOM_CONFIG

    /**
     * Replace the configuration of a room whole, and keep it across restarts,
     * when the product declares one with `setRoomConfig`. A room the product
     * declared none for has [HOST_DEFAULT_CHAT_ROOM_CONFIG]. A change of
     * `productSource` discards the stored messages or the cached pages of the
     * room.
     */
    @Throws(HostRejection::class)
    suspend fun setRoomConfig(roomId: String, config: ChatRoomConfig) {
        if (config != HOST_DEFAULT_CHAT_ROOM_CONFIG) throw unservedChatRequest("setRoomConfig")
    }

    /** Replace the name and description of a room. */
    @Throws(HostRejection::class)
    suspend fun setRoomMetadata(roomId: String, metadata: ChatRoomMetadata) {}

    /** Replace how a room shows in the chat list. */
    @Throws(HostRejection::class)
    suspend fun setRoomPreview(roomId: String, preview: ChatRoomPreview): Unit =
        throw unservedChatRequest("setRoomPreview")

    /**
     * Delete a room with its messages, cache and declarations, and close its
     * screen. A room that does not exist is not an error.
     */
    @Throws(HostRejection::class)
    suspend fun deleteRoom(roomId: String): Unit = throw unservedChatRequest("deleteRoom")

    /**
     * Apply one change to the messages of a room. With [persist] the host
     * stores the change. Without it the host applies the change only to an
     * open room screen and to the chat list preview.
     *
     * Throw [NativeChatFeedRejection] to refuse the change. The product
     * receives each case as its own error, and any other exception as
     * [NativeChatFeedRejection.Rejected].
     */
    @Throws(NativeChatFeedRejection::class)
    suspend fun updateFeed(roomId: String, update: ChatFeedUpdate, persist: Boolean): Unit =
        throw NativeChatFeedRejection.Rejected(unservedChatRequestReason("updateFeed"))

    // The members below have defaults for a host that does not serve the Chat
    // Extension v2 additions: it lists no feature, reads default declarations
    // and refuses each change.

    /**
     * Return the names of the Chat features that this host serves from end to
     * end, from the feature table of the Chat Extension v2 additions RFC. The
     * core removes each name that it does not know or cannot back. List a name
     * only in the build that serves its feature.
     */
    @Throws(HostRejection::class)
    suspend fun capabilities(): List<String> = emptyList()

    /**
     * Return the optional features the product last declared for a room, or
     * [HOST_DEFAULT_CHAT_ROOM_FEATURES] when it declared none.
     */
    @Throws(HostRejection::class)
    suspend fun roomFeatures(roomId: String): ChatRoomFeatures = HOST_DEFAULT_CHAT_ROOM_FEATURES

    /**
     * Replace the optional features of a room whole, and keep them across
     * restarts. Keep a feature that this host does not serve, so that a later
     * build applies it.
     */
    @Throws(HostRejection::class)
    suspend fun setRoomFeatures(roomId: String, features: ChatRoomFeatures): Unit =
        throw unservedChatRequest("setRoomFeatures")

    /**
     * Show or change the call of a room, or remove it with `null`. The core
     * has already checked that this host lists the feature of the call.
     */
    @Throws(HostRejection::class)
    suspend fun setRoomCall(roomId: String, call: ChatRoomCall?): Unit =
        throw unservedChatRequest("setRoomCall")

    /**
     * Set the contact of a room and keep it across restarts. Draw the room as
     * the host's own person-to-person chat with that contact. The core has
     * already checked that this host lists `contact_rooms`.
     */
    @Throws(HostRejection::class)
    suspend fun setRoomContact(roomId: String, contact: ChatRoomContact): Unit =
        throw unservedChatRequest("setRoomContact")
}

/**
 * The features of a room that declares none: the room shows what a room of
 * Chat Extension v2 shows.
 */
val HOST_DEFAULT_CHAT_ROOM_FEATURES: ChatRoomFeatures =
    ChatRoomFeatures(calls = null, hopAttachments = false)

/**
 * The configuration of a room that declares nothing: the host stores the
 * messages and serves every part of the room itself.
 */
val HOST_DEFAULT_CHAT_ROOM_CONFIG: ChatRoomConfig =
    ChatRoomConfig(productSource = false, reactionDetails = false, reportSeen = false)

private fun unservedChatRequestReason(request: String): String = "this host does not serve chat $request"

private fun unservedChatRequest(request: String): HostRejection =
    HostRejection.Rejected(unservedChatRequestReason(request))

/**
 * Native Pocket collection surface. Implement and pass to
 * [TrUAPIHostRuntime.openProductExecution] when the host has a Pocket surface;
 * hosts without one pass nothing.
 *
 * Threading: [listCards] runs inline on the core runtime shared by every
 * product execution, so it must be safe to enter concurrently and return
 * promptly. [removeCard] suspends, so it can wait on the host's storage.
 */
interface PocketHostBridge {
    /**
     * Return the product's cards as this host holds them, each carrying
     * whether the host pinned it.
     */
    @Throws(HostRejection::class)
    fun listCards(): List<PocketCard>

    /**
     * Remove one of the product's cards and report what happened. Decide and
     * remove together, under whatever lock this host holds, so a card cannot
     * be pinned between the two.
     */
    @Throws(HostRejection::class)
    suspend fun removeCard(cardId: String): NativePocketRemoval
}

/**
 * Native game-reminder surface. Implement and pass to
 * [TrUAPIHostRuntime.openProductExecution] when the host can hold reminders;
 * hosts without one pass nothing.
 *
 * The host holds one reminder per product: a schedule replaces the reminder
 * the same product already holds. The core asks for no per-product consent;
 * the host asks the OS for what it needs, rings an alarm where the OS allows
 * one and delivers a notification otherwise, may add the game to the
 * calendar, keeps the reminder across app kill and reboot, and drops it once
 * the game starts.
 *
 * Threading: both calls suspend, so an implementation may switch to its own
 * dispatcher to answer; implementations must be safe to enter concurrently.
 */
interface GameHostBridge {
    /**
     * Hold [startsAt] (Unix milliseconds, UTC) as this product's reminder, replacing any it holds.
     * Any exception, including an OS that allows neither alarms nor notifications, reaches the
     * product as a host failure carrying its reason.
     */
    @Throws(HostRejection::class)
    suspend fun scheduleReminder(startsAt: ULong)

    /** Drop this product's reminder. Dropping none succeeds. */
    @Throws(HostRejection::class)
    suspend fun cancelReminder()
}

/**
 * Adapter from the public [HostBridge] surface to the generated UniFFI
 * [HostCallbacks] interface. Keeps the public API stable even if uniffi-bindgen
 * renames generated symbols.
 */
private class HostCallbackAdapter(private val bridge: HostBridge) : HostCallbacks {
    // The core declares this and `authStateChanged` infallible, so uniffi has
    // no error type to convert a throw into and panics -- which aborts under
    // `panic = "abort"`. Neither may let a host exception reach the FFI.
    override fun onCoreLog(marker: String, detail: String) {
        runCatching { bridge.onCoreLog(marker, detail) }
    }

    // Infallible across the FFI for the same reason `onCoreLog` is.
    override fun workerDemandChanged(productId: String, transition: WorkerTransition) {
        runCatching { bridge.workerDemandChanged(productId, transition) }
    }

    // Infallible across the FFI for the same reason `onCoreLog` is.
    override fun devicePaired(device: PairedSsoPeer) {
        runCatching { bridge.devicePaired(device) }
    }

    // Infallible across the FFI for the same reason `onCoreLog` is.
    override fun durableWorkChanged(pending: Boolean) {
        runCatching { bridge.durableWorkChanged(pending) }
    }

    override suspend fun navigateTo(url: String) =
        withNavigateRejection { bridge.navigateTo(url) }

    override suspend fun pushNotification(request: HostPushNotificationRequest): UInt =
        withHostRejection { bridge.pushNotification(request) }

    override fun cancelNotification(id: UInt) =
        withHostRejection { bridge.cancelNotification(id) }

    override suspend fun devicePermission(
        product: ProductExecutionConfig,
        request: HostDevicePermissionRequest,
    ): PermissionDecision =
        withHostRejection {
            bridge.devicePermission(product, request)
        }

    override suspend fun devicePermissionStatus(
        request: HostDevicePermissionRequest,
    ): DevicePermissionStatus = withHostRejection { bridge.devicePermissionStatus(request) }

    override suspend fun setExpandedCardFaceShown(shown: Boolean): ExpandedCardFaceOutcome =
        withHostRejection { bridge.setExpandedCardFaceShown(shown) }

    override suspend fun remotePermission(
        product: ProductExecutionConfig,
        request: RemotePermission,
    ): PermissionDecision =
        withHostRejection {
            bridge.remotePermission(product, request)
        }

    override fun authStateChanged(state: AuthState) {
        try {
            bridge.authStateChanged(state)
        } catch (error: Throwable) {
            runCatching {
                bridge.onCoreLog("host.auth_state_changed.threw", error.stackTraceToString())
            }
        }
    }

    override suspend fun coreStorageRead(key: ByteArray): ByteArray? =
        withHostRejection { bridge.coreStorage.read(key) }

    override suspend fun coreStorageWrite(key: ByteArray, value: ByteArray) =
        withHostRejection { bridge.coreStorage.write(key, value) }

    override suspend fun coreStorageClear(key: ByteArray) =
        withHostRejection { bridge.coreStorage.clear(key) }

    override fun chainConnect(genesisHash: ByteArray): UInt? =
        withHostRejection { bridge.chainConnect(genesisHash) }

    override fun chainSend(connectionId: UInt, request: String) =
        withHostRejection { bridge.chainSend(connectionId, request) }

    override fun chainClose(connectionId: UInt) =
        withHostRejection { bridge.chainClose(connectionId) }

    override suspend fun confirmUserAction(review: UserConfirmationReview): Boolean =
        withHostRejection { bridge.confirmUserAction(review) }

    override suspend fun confirmPermission(review: UserConfirmationReview): PermissionDecision =
        withHostRejection { bridge.confirmPermission(review) }

    override suspend fun lookupPreimage(key: ByteArray): ByteArray? =
        withHostRejection { bridge.lookupPreimage(key) }

    override fun currentTheme(): HostThemeSubscribeItem =
        withHostRejection { bridge.currentTheme() }

    override fun currentLocale(): HostLocaleSubscribeItem =
        withHostRejection { bridge.currentLocale() }

    override suspend fun featureSupported(request: HostFeatureSupportedRequest): Boolean =
        withHostRejection { bridge.featureSupported(request) }

    override fun supportedChains(): HostChainSet =
        withHostRejection { bridge.supportedChains() }

    override suspend fun localStorageRead(key: String): ByteArray? =
        withStorageException { bridge.storage.read(key) }

    override suspend fun localStorageWrite(key: String, value: ByteArray) =
        withStorageException { bridge.storage.write(key, value) }

    override suspend fun localStorageClear(key: String) =
        withStorageException { bridge.storage.clear(key) }

    override suspend fun beginOperation(productId: String, label: String): UInt =
        withHostRejection { bridge.beginOperation(productId, label) }

    override suspend fun endOperation(productId: String, id: UInt) =
        withHostRejection { bridge.endOperation(productId, id) }
}

// A host that throws an exception type its callback does not declare crosses
// the FFI as an unexpected callback error. The Rust core converts those rather
// than aborting, but the reason it receives is then a raw JVM description, so
// each adapter funnels host throws into the declared type here.

// Bounded: this reaches the product as the rejection reason, and a host
// message can carry a whole failed statement.
private fun hostRejectionReason(error: Throwable): String =
    (error.message ?: error.toString()).take(HOST_REJECTION_REASON_MAX_CHARS)

private const val HOST_REJECTION_REASON_MAX_CHARS = 256

private inline fun <T> withHostRejection(operation: () -> T): T =
    try {
        operation()
    } catch (rejection: HostRejection) {
        throw rejection
    } catch (cancellation: CancellationException) {
        throw cancellation
    } catch (error: Throwable) {
        throw HostRejection.Rejected(hostRejectionReason(error)).apply { initCause(error) }
    }

private inline fun <T> withNavigateRejection(operation: () -> T): T =
    try {
        operation()
    } catch (rejection: HostNavigateToException) {
        throw rejection
    } catch (cancellation: CancellationException) {
        throw cancellation
    } catch (error: Throwable) {
        throw HostNavigateToException.Unknown(hostRejectionReason(error))
            .apply { initCause(error) }
    }

private inline fun <T> withStorageException(operation: () -> T): T =
    try {
        operation()
    } catch (storage: HostLocalStorageReadException) {
        throw storage
    } catch (cancellation: CancellationException) {
        throw cancellation
    } catch (error: Throwable) {
        throw HostLocalStorageReadException.Unknown(hostRejectionReason(error))
            .apply { initCause(error) }
    }

/**
 * Adapter from the public [ChatHostBridge] surface to the generated UniFFI
 * [NativeChatCallbacks] interface.
 */
private class ChatCallbackAdapter(private val bridge: ChatHostBridge) : NativeChatCallbacks {
    override suspend fun createRoom(
        roomId: String,
        name: String,
        icon: String,
    ): ChatRoomRegistrationStatus = withHostRejection { bridge.createRoom(roomId, name, icon) }

    override suspend fun registerBot(
        botId: String,
        name: String,
        icon: String,
    ): ChatBotRegistrationStatus = withHostRejection { bridge.registerBot(botId, name, icon) }

    override suspend fun postMessage(roomId: String, content: ChatMessageContent, alt: String?): String =
        withHostRejection { bridge.postMessage(roomId, content, alt) }

    override suspend fun setRoomFooter(roomId: String, footer: ChatRoomFooter) =
        withHostRejection { bridge.setRoomFooter(roomId, footer) }

    override suspend fun listRooms(): List<ChatRoom> = withHostRejection { bridge.listRooms() }

    override suspend fun roomConfig(roomId: String): ChatRoomConfig =
        withHostRejection { bridge.roomConfig(roomId) }

    override suspend fun setRoomConfig(roomId: String, config: ChatRoomConfig) =
        withHostRejection { bridge.setRoomConfig(roomId, config) }

    override suspend fun setRoomMetadata(roomId: String, metadata: ChatRoomMetadata) =
        withHostRejection { bridge.setRoomMetadata(roomId, metadata) }

    override suspend fun setRoomPreview(roomId: String, preview: ChatRoomPreview) =
        withHostRejection { bridge.setRoomPreview(roomId, preview) }

    override suspend fun deleteRoom(roomId: String) = withHostRejection { bridge.deleteRoom(roomId) }

    override suspend fun updateFeed(roomId: String, update: ChatFeedUpdate, persist: Boolean) =
        withFeedRejection { bridge.updateFeed(roomId, update, persist) }

    override suspend fun capabilities(): List<String> = withHostRejection { bridge.capabilities() }

    override suspend fun roomFeatures(roomId: String): ChatRoomFeatures =
        withHostRejection { bridge.roomFeatures(roomId) }

    override suspend fun setRoomFeatures(roomId: String, features: ChatRoomFeatures) =
        withHostRejection { bridge.setRoomFeatures(roomId, features) }

    override suspend fun setRoomCall(roomId: String, call: ChatRoomCall?) =
        withHostRejection { bridge.setRoomCall(roomId, call) }

    override suspend fun setRoomContact(roomId: String, contact: ChatRoomContact) =
        withHostRejection { bridge.setRoomContact(roomId, contact) }

    /**
     * Pass a [NativeChatFeedRejection] to the product as it is, and any other
     * exception as [NativeChatFeedRejection.Rejected].
     */
    private suspend fun withFeedRejection(operation: suspend () -> Unit) =
        try {
            operation()
        } catch (rejection: NativeChatFeedRejection) {
            throw rejection
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (error: Throwable) {
            throw NativeChatFeedRejection.Rejected(hostRejectionReason(error)).apply { initCause(error) }
        }
}

/**
 * Adapter from the public [PocketHostBridge] surface to the generated UniFFI
 * [NativePocketCallbacks] interface.
 */
/**
 * Native contacts surface: the user's contact list, and the picker drawn over
 * it. Install with [TrUAPIHostRuntime.setContacts], once, because the list
 * belongs to the host and not to any one product. A runtime without one
 * answers `contacts.pick` with `Unsupported`.
 *
 * Nothing here reaches a product, and the list never reaches the core: it
 * asks only about the handles a transaction names, and the picker returns the
 * one person the user chose. Omit the contacts the user has blocked, from both.
 */
interface ContactsHostBridge {
    /**
     * Resolve [HostContactLookup.handles] to contacts: one entry per handle, in
     * order, `null` where none matches. A contact's handle is BLAKE2b-256 keyed
     * with [HostContactLookup.handleKey] over its 32-byte account. Runs inline,
     * so answer from what is already in hand.
     */
    @Throws(HostRejection::class)
    fun contacts(lookup: HostContactLookup): HostContactMatches

    /**
     * Present the picker on behalf of [productId] and report what the user
     * did. With no contacts, answer [HostContactPick.NoContacts] instead of
     * drawing an empty overlay.
     */
    @Throws(HostRejection::class)
    suspend fun pickContact(productId: String): HostContactPick
}

private class ContactsCallbackAdapter(private val bridge: ContactsHostBridge) : NativeContactsCallbacks {
    override fun contacts(lookup: HostContactLookup): HostContactMatches =
        withHostRejection { bridge.contacts(lookup) }

    override suspend fun pickContact(productId: String): HostContactPick =
        withHostRejection { bridge.pickContact(productId) }
}

/**
 * Draws the viewfinder for `scanner.scan`, following the rules on the core's
 * `ScannerPlatform`. Closes it when the coroutine is cancelled.
 */
interface ScannerHostBridge {
    @Throws(HostRejection::class)
    suspend fun scanCode(
        productId: String,
        executionKind: ProductExecutionKind,
        request: HostScannerScanRequest,
    ): HostScan
}

private class ScannerCallbackAdapter(private val bridge: ScannerHostBridge) : NativeScannerCallbacks {
    override suspend fun scanCode(
        productId: String,
        executionKind: ProductExecutionKind,
        request: HostScannerScanRequest,
    ): HostScan = withHostRejection { bridge.scanCode(productId, executionKind, request) }
}

/**
 * The person chat services of the host, following the rules on the core's
 * `PersonChatPlatform`: the `wss://` URLs of the HOP nodes of the network's
 * Bulletin chain, the devices paired with the wallet, the chat's contacts
 * for the rest of the app, and the claim of the coins contacts pay with.
 */
interface PersonChatHostBridge {
    @Throws(HostRejection::class)
    suspend fun hopNodes(): List<String>

    /** The devices paired with this wallet device over SSO, as the chat announces them to contacts. */
    @Throws(HostRejection::class)
    suspend fun pairedDevices(): List<HostPairedDevice>

    /** Replace the chat contacts of [productId], which the app shows outside the chat. */
    @Throws(HostRejection::class)
    suspend fun setContacts(productId: String, contacts: List<HostChatContact>)

    /**
     * At most [maxLen] bytes from [offset] of the native chat history staged for [productId], and the size of the
     * whole export; a total of 0 when none is staged.
     */
    @Throws(HostRejection::class)
    suspend fun historyExport(productId: String, offset: ULong, maxLen: UInt): HostHistoryExportChunk

    /** [productId] imported the staged history: drop it. */
    @Throws(HostRejection::class)
    suspend fun historyImported(productId: String)

    /**
     * Take the coin keys of a payment a contact sent in a room of [productId], and claim the coins with the app's
     * coinage flow keyed by the message id of [payment], from which the app also draws the claim state of the room's
     * payment message. Idempotent by message id. An app without coinage keeps this default, and the chat product
     * keeps the keys.
     */
    @Throws(HostRejection::class)
    suspend fun paymentReceived(
        productId: String,
        payment: HostPersonChatPaymentReceivedRequest,
    ): HostChatPaymentReceipt = HostChatPaymentReceipt.UNSUPPORTED
}

private class PersonChatCallbackAdapter(private val bridge: PersonChatHostBridge) : NativePersonChatCallbacks {
    override suspend fun hopNodes(): List<String> = withHostRejection { bridge.hopNodes() }

    override suspend fun pairedDevices(): List<HostPairedDevice> = withHostRejection { bridge.pairedDevices() }

    override suspend fun setContacts(productId: String, contacts: List<HostChatContact>) =
        withHostRejection { bridge.setContacts(productId, contacts) }

    override suspend fun historyExport(productId: String, offset: ULong, maxLen: UInt): HostHistoryExportChunk =
        withHostRejection { bridge.historyExport(productId, offset, maxLen) }

    override suspend fun historyImported(productId: String) = withHostRejection { bridge.historyImported(productId) }

    override suspend fun paymentReceived(
        productId: String,
        payment: HostPersonChatPaymentReceivedRequest,
    ): HostChatPaymentReceipt = withHostRejection { bridge.paymentReceived(productId, payment) }
}

/**
 * The push service of the host, following the rules on the core's
 * `PushPlatform`: the push relay, the push tokens of the device, and the
 * chat push routes the host shows pushes with while no chat worker runs. Only the core calls it.
 */
interface PushHostBridge {
    /** Give [delivery] to the push relay. Returns whether the push service took it. */
    @Throws(HostRejection::class)
    suspend fun sendPush(productId: String, delivery: HostPushDelivery): Boolean

    /** The push tokens of this device now, which the chat product shares with its contacts. */
    @Throws(HostRejection::class)
    suspend fun pushTokens(): HostPushTokens

    /** Replace the chat push routes of [productId] with [routes]. */
    @Throws(HostRejection::class)
    suspend fun setChatPushRoutes(productId: String, routes: List<HostChatPushRoute>)
}

private class PushCallbackAdapter(private val bridge: PushHostBridge) : NativePushCallbacks {
    override suspend fun sendPush(productId: String, delivery: HostPushDelivery): Boolean =
        withHostRejection { bridge.sendPush(productId, delivery) }

    override suspend fun pushTokens(): HostPushTokens = withHostRejection { bridge.pushTokens() }

    override suspend fun setChatPushRoutes(productId: String, routes: List<HostChatPushRoute>) =
        withHostRejection { bridge.setChatPushRoutes(productId, routes) }
}

/**
 * The call engine of the host for the calls of product rooms, following the
 * rules on the core's `CallMediaPlatform`. The product carries the signals
 * between the two hosts: the engine's own signals go back to it as the room
 * event `CallSignal`. Only the core calls it.
 */
interface CallMediaHostBridge {
    /** Start the media of [callId] of [roomId] of [productId]. */
    @Throws(HostRejection::class)
    suspend fun startCallMedia(productId: String, roomId: String, callId: String, video: Boolean, offerer: Boolean)

    /** A media signal of the other side of [callId]. */
    @Throws(HostRejection::class)
    suspend fun callMediaSignal(productId: String, roomId: String, callId: String, signal: ByteArray)

    /** Stop the media of [callId]. */
    @Throws(HostRejection::class)
    suspend fun stopCallMedia(productId: String, roomId: String, callId: String)
}

private class CallMediaCallbackAdapter(private val bridge: CallMediaHostBridge) : NativeCallMediaCallbacks {
    override suspend fun startCallMedia(productId: String, roomId: String, callId: String, video: Boolean, offerer: Boolean) =
        withHostRejection { bridge.startCallMedia(productId, roomId, callId, video, offerer) }

    override suspend fun callMediaSignal(productId: String, roomId: String, callId: String, signal: ByteArray) =
        withHostRejection { bridge.callMediaSignal(productId, roomId, callId, signal) }

    override suspend fun stopCallMedia(productId: String, roomId: String, callId: String) =
        withHostRejection { bridge.stopCallMedia(productId, roomId, callId) }
}

private class PocketCallbackAdapter(private val bridge: PocketHostBridge) : NativePocketCallbacks {
    override fun listCards(): List<PocketCard> = withHostRejection { bridge.listCards() }

    override suspend fun removeCard(cardId: String): NativePocketRemoval =
        withHostRejection { bridge.removeCard(cardId) }
}

/**
 * Adapter from the public [GameHostBridge] surface to the generated UniFFI
 * [NativeGameCallbacks] interface.
 */
private class GameCallbackAdapter(private val bridge: GameHostBridge) : NativeGameCallbacks {
    override suspend fun scheduleReminder(startsAt: ULong) = withHostRejection { bridge.scheduleReminder(startsAt) }

    override suspend fun cancelReminder() = withHostRejection { bridge.cancelReminder() }
}

/**
 * Native top-up engine, which claims a source's funds into the user's balance.
 * Install with [TrUAPIHostRuntime.setTopUp], once. A runtime without one
 * answers top-ups with `Unsupported`.
 *
 * The core has already checked the session, the source keys and the amount,
 * and hashed the product into every id it passes, so ids never collide across
 * products. Report each status after the one [topUpStatus] answered with
 * [TrUAPIHostRuntime.notifyTopUpStatus]; a terminal one ends the stream.
 */
interface TopUpHostBridge {
    /**
     * Start top-up [request] for [productId], returning once the host has
     * accepted it. Its amount is a decimal string of CASH units.
     */
    @Throws(HostPaymentTopUpException::class)
    suspend fun topUp(productId: String, request: HostPaymentTopUpRequest)

    /**
     * The current status of [productId]'s top-up [id], or `null` when the host
     * holds no such top-up. Runs inline on the core's thread.
     */
    @Throws(HostRejection::class)
    fun topUpStatus(productId: String, id: ByteArray): HostPaymentTopUpStatusSubscribeItem?
}

/**
 * Native payment engine, which pays from the user's balance to an account.
 * Install with [TrUAPIHostRuntime.setPayments], once. A runtime without one
 * answers payment requests with `Unsupported`.
 *
 * The core hashes the product into every id it passes. Answer
 * `InsufficientBalance` whenever the balance is short: the core turns it into
 * `Rejected` for a product without balance access. Report each status after
 * the one [paymentStatus] answered with [TrUAPIHostRuntime.notifyPaymentStatus].
 */
interface PaymentHostBridge {
    /**
     * Ask the user to approve payment [request] for [productId], returning once
     * they have decided and the host took it on. Its amount is a decimal string
     * of CASH units.
     */
    @Throws(HostPaymentException::class)
    suspend fun requestPayment(productId: String, request: HostPaymentRequest)

    /**
     * The current status of [productId]'s payment [id], or `null` when the host
     * holds no such payment. Runs inline on the core's thread.
     */
    @Throws(HostRejection::class)
    fun paymentStatus(productId: String, id: ByteArray): HostPaymentStatusSubscribeItem?
}

/**
 * Native balance view, which shares what a payment can spend with a product.
 * Install with [TrUAPIHostRuntime.setBalance], once, and report each change
 * with [TrUAPIHostRuntime.notifyBalance]. The core owns balance access, so do
 * not prompt for it here.
 */
interface BalanceHostBridge {
    /**
     * The user's balance in [purse] (`null` for the main purse), as a decimal
     * string of CASH units: what a payment request can spend now. It is the same
     * for every product; [productId] only names the one asking.
     */
    @Throws(HostPaymentBalanceSubscribeException::class)
    suspend fun balance(productId: String, purse: UInt?): String
}

private class TopUpCallbackAdapter(private val bridge: TopUpHostBridge) : NativeTopUpCallbacks {
    override suspend fun topUp(productId: String, request: HostPaymentTopUpRequest) =
        try {
            bridge.topUp(productId, request)
        } catch (error: HostPaymentTopUpException) {
            throw error
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (error: Throwable) {
            throw HostPaymentTopUpException.Unknown(hostRejectionReason(error)).apply { initCause(error) }
        }

    override fun topUpStatus(productId: String, id: ByteArray): HostPaymentTopUpStatusSubscribeItem? =
        withHostRejection { bridge.topUpStatus(productId, id) }
}

private class PaymentCallbackAdapter(private val bridge: PaymentHostBridge) : NativePaymentCallbacks {
    override suspend fun requestPayment(productId: String, request: HostPaymentRequest) =
        try {
            bridge.requestPayment(productId, request)
        } catch (error: HostPaymentException) {
            throw error
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (error: Throwable) {
            throw HostPaymentException.Unknown(hostRejectionReason(error)).apply { initCause(error) }
        }

    override fun paymentStatus(productId: String, id: ByteArray): HostPaymentStatusSubscribeItem? =
        withHostRejection { bridge.paymentStatus(productId, id) }
}

private class BalanceCallbackAdapter(private val bridge: BalanceHostBridge) : NativeBalanceCallbacks {
    override suspend fun balance(productId: String, purse: UInt?): String =
        try {
            bridge.balance(productId, purse)
        } catch (error: HostPaymentBalanceSubscribeException) {
            throw error
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (error: Throwable) {
            throw HostPaymentBalanceSubscribeException.Unknown(hostRejectionReason(error))
                .apply { initCause(error) }
        }
}

/**
 * Bootstrap helper for the native localhost WebSocket bridge that a product
 * execution starts.
 */
object LocalhostBridgeBootstrap {
    /**
     * Supplies the WebSocket endpoint to the shared browser container.
     * Inject at document start, before the container and product scripts.
     */
    fun script(port: UShort, token: String): String =
        localhostBridgeBootstrapScript(port = port, token = token)
}

/**
 * Process-owned Rust host runtime. Product executables open independent
 * connections from this object and share its authentication and core services.
 */
class TrUAPIHostRuntime @Throws(NativeRuntimeConfigException::class) constructor(
    bridge: HostBridge,
    runtimeConfig: HostRuntimeConfig,
) : AutoCloseable {
    // Co-owns the adapter alongside the generated FfiConverter handle map,
    // which is what actually keeps the callback object alive for the runtime.
    private val callbackRetainer: HostCallbacks = HostCallbackAdapter(bridge)
    private val inner: NativeTrUApiHostRuntime =
        NativeTrUApiHostRuntime.withRuntimeConfig(callbackRetainer, runtimeConfig)

    // Co-owns the contacts adapter for as long as the runtime holds it.
    private var contactsRetainer: NativeContactsCallbacks? = null

    /**
     * Install the host's contacts adapter, which owns the contact list and
     * draws the picker.
     *
     * Set-once, so the picker cannot change hands under a running product.
     * Returns whether this call installed it. Call it before opening any
     * product execution.
     */
    fun setContacts(contacts: ContactsHostBridge): Boolean {
        val adapter = ContactsCallbackAdapter(contacts)
        contactsRetainer = adapter
        return inner.setContactsCallbacks(adapter)
    }

    // Co-owns the scanner adapter for as long as the runtime holds it.
    private var scannerRetainer: NativeScannerCallbacks? = null

    /**
     * Install the host's scanner before opening any product execution.
     * Set-once: returns whether this call installed it.
     */
    fun setScanner(scanner: ScannerHostBridge): Boolean {
        val adapter = ScannerCallbackAdapter(scanner)
        scannerRetainer = adapter
        return inner.setScannerCallbacks(adapter)
    }

    // Co-owns the push adapter for as long as the runtime holds it.
    private var pushRetainer: NativePushCallbacks? = null

    /**
     * Install the host's push service before opening any product execution.
     * Set-once: returns whether this call installed it.
     */
    fun setPush(push: PushHostBridge): Boolean {
        val adapter = PushCallbackAdapter(push)
        pushRetainer = adapter
        return inner.setPushCallbacks(adapter)
    }

    // Co-owns the call media adapter for as long as the runtime holds it.
    private var callMediaRetainer: NativeCallMediaCallbacks? = null

    /**
     * Install the host's call engine for the calls of product rooms before
     * opening any product execution. Set-once: returns whether this call
     * installed it.
     */
    fun setCallMedia(callMedia: CallMediaHostBridge): Boolean {
        val adapter = CallMediaCallbackAdapter(callMedia)
        callMediaRetainer = adapter
        return inner.setCallMediaCallbacks(adapter)
    }

    // Co-owns the person chat adapter for as long as the runtime holds it.
    private var personChatRetainer: NativePersonChatCallbacks? = null

    /**
     * Install the host's person chat services (the HOP nodes) before opening
     * any product execution. Set-once: returns whether this call installed it.
     */
    fun setPersonChat(personChat: PersonChatHostBridge): Boolean {
        val adapter = PersonChatCallbackAdapter(personChat)
        personChatRetainer = adapter
        return inner.setPersonChatCallbacks(adapter)
    }

    /**
     * Keep [secret], the X25519 key the native chat of this install used as
     * its device key, so the chat product goes on using it and contacts keep
     * reaching this device. Call it once, when the app moves its native chat
     * to the chat product, before the product first runs. Returns false when
     * the core already keeps a different key, which it never replaces.
     */
    @Throws(HostRejection::class)
    suspend fun importChatDeviceKey(secret: ByteArray): Boolean = inner.importChatDeviceKey(secret)

    /**
     * Tell the core the host's contacts changed. Call it whenever a contact is
     * removed or blocked, so a contact handle the core cached stops resolving.
     */
    fun notifyContactsChanged() {
        inner.notifyContactsChanged()
    }

    // Co-own the payment adapters for as long as the runtime holds them.
    private var topUpRetainer: NativeTopUpCallbacks? = null
    private var paymentRetainer: NativePaymentCallbacks? = null
    private var balanceRetainer: NativeBalanceCallbacks? = null

    /**
     * Install the host's top-up engine. Set-once; returns whether this call
     * installed it. Call it before opening any product execution.
     */
    fun setTopUp(topUp: TopUpHostBridge): Boolean {
        val adapter = TopUpCallbackAdapter(topUp)
        topUpRetainer = adapter
        return inner.setTopUpCallbacks(adapter)
    }

    /** Report a later status of [productId]'s top-up [id] to the products following it. */
    fun notifyTopUpStatus(productId: String, id: ByteArray, status: HostPaymentTopUpStatusSubscribeItem) {
        inner.notifyTopUpStatus(productId, id, status)
    }

    /**
     * Install the host's payment engine. Set-once; returns whether this call
     * installed it. Call it before opening any product execution.
     */
    fun setPayments(payments: PaymentHostBridge): Boolean {
        val adapter = PaymentCallbackAdapter(payments)
        paymentRetainer = adapter
        return inner.setPaymentCallbacks(adapter)
    }

    /** Report a later status of [productId]'s payment [id] to the products following it. */
    fun notifyPaymentStatus(productId: String, id: ByteArray, status: HostPaymentStatusSubscribeItem) {
        inner.notifyPaymentStatus(productId, id, status)
    }

    /**
     * Install the host's balance view. Set-once; returns whether this call
     * installed it. Call it before opening any product execution.
     */
    fun setBalance(balance: BalanceHostBridge): Boolean {
        val adapter = BalanceCallbackAdapter(balance)
        balanceRetainer = adapter
        return inner.setBalanceCallbacks(adapter)
    }

    /**
     * Report the new balance of [purse] (`null` for the main purse), a decimal
     * string of CASH units, to the products following it.
     */
    fun notifyBalance(purse: UInt?, available: String) {
        inner.notifyBalance(purse, available)
    }

    /**
     * Open one executable connection with a host-assigned immutable context.
     * Pass [chat] to install the host's Chat adapter; hosts without the Chat
     * modality omit it. Pass [pocket] to install the card collection, and omit
     * that where the host has no Pocket surface. Pass [game] to hold game
     * reminders, and omit it where the host cannot.
     */
    @Throws(NativeRuntimeConfigException::class)
    fun openProductExecution(
        bridge: HostBridge,
        configuration: ProductExecutionConfig,
        chat: ChatHostBridge? = null,
        pocket: PocketHostBridge? = null,
        game: GameHostBridge? = null,
    ): TrUAPIProductExecution {
        val adapter = HostCallbackAdapter(bridge)
        val chatAdapter = chat?.let { ChatCallbackAdapter(it) }
        val pocketAdapter = pocket?.let { PocketCallbackAdapter(it) }
        val gameAdapter = game?.let { GameCallbackAdapter(it) }
        val execution =
            inner.openProductExecution(
                adapter,
                chatAdapter,
                pocketAdapter,
                gameAdapter,
                configuration,
            )
        return TrUAPIProductExecution(execution, adapter, chatAdapter, pocketAdapter, gameAdapter)
    }

    /**
     * Take one reference on the product's worker for a modality holder that is
     * on screen or in flight. The first one reports a start transition to
     * [HostBridge.workerDemandChanged], which is where the host starts the
     * worker. Pair every call with one [releaseWorker].
     */
    fun acquireWorker(productId: String) {
        inner.acquireWorker(productId)
    }

    /**
     * Release one reference. The last one reports a stop transition, after
     * which the host may stop the worker. Releasing with none held is a no-op.
     */
    fun releaseWorker(productId: String) {
        inner.releaseWorker(productId)
    }

    /**
     * Tell the core if the app is in the foreground. While it is, the core
     * holds the worker of each product that declares
     * [WorkerKeepAlive.FOREGROUND]. The core starts in the background, so
     * the host reports the state when it builds the runtime and at each
     * change.
     */
    fun setAppForeground(foreground: Boolean) {
        inner.setAppForeground(foreground)
    }

    /**
     * Tell the core when the worker of a product runs, from the `keepAlive`
     * field of its Worker executable manifest. The host sets
     * [WorkerKeepAlive.ON_DEMAND] again when the product goes.
     */
    fun setWorkerKeepAlive(productId: String, keepAlive: WorkerKeepAlive) {
        inner.setWorkerKeepAlive(productId, keepAlive)
    }

    /**
     * Send one event about a room to the product: the user pressed a call
     * button, answered or ended a call, or the call has a media signal. The
     * core keeps the newest events of a product
     * until its worker subscribes, so the worker does not have to run.
     */
    @Throws(HostRejection::class)
    fun publishChatRoomEvent(productId: String, roomId: String, event: ChatRoomEvent) {
        inner.publishChatRoomEvent(productId, roomId, event)
    }

    /**
     * Hold the worker of the product for a wake of 30 seconds. A new wake moves
     * the end of the hold. A product gets at most 12 notification wakes in one
     * hour; after that the core answers [ChatWakeOutcome.BudgetSpent].
     */
    @Throws(HostRejection::class)
    fun wakeProduct(productId: String, reason: ChatWakeReason): ChatWakeOutcome =
        inner.wakeProduct(productId, reason)

    /**
     * Upload a user file of a room of the chat product that declares
     * `hop_attachments` to a HOP node, up to 128 MiB, and give the
     * `ChatResource.Hop` to send to the product. Throws
     * [NativeChatStageRejection]: `NotDeclared` for a room that does not
     * declare `hop_attachments` or another product, `TooLarge` above
     * 128 MiB, and `SubmitFailed`. [progress] gets the bytes the node holds
     * and the size of the file: none before the first part goes, then after
     * each part, and all of them once the file is in.
     */
    @Throws(NativeChatStageRejection::class)
    suspend fun hopChatAttachment(
        productId: String,
        roomId: String,
        bytes: ByteArray,
        progress: ((sent: ULong, total: ULong) -> Unit)? = null,
    ): ChatResource =
        inner.chatHopAttachment(
            productId,
            roomId,
            bytes,
            progress?.let { report ->
                object : NativeHopUploadProgress {
                    // A throw here would panic the core, as in [render].
                    override fun onProgress(sent: ULong, total: ULong) {
                        runCatching { report(sent, total) }
                    }
                }
            },
        )

    /**
     * The bytes of a `ChatResource.Hop` of a room of the chat product, which
     * the core claims from the node it names when [PersonChatHostBridge]
     * lists that node, or from Bulletin once the node promoted it. Keep the
     * bytes, then call [ackHopChatResource] with the entries. [progress] gets
     * the bytes received and the size of the file: once the size is known,
     * then after each part of the file.
     */
    @Throws(HostRejection::class)
    suspend fun openHopChatResource(
        productId: String,
        resource: ChatResource,
        progress: ((received: ULong, total: ULong) -> Unit)? = null,
    ): NativeHopFile =
        inner.openHopChatResource(
            productId,
            resource,
            progress?.let { report ->
                object : NativeHopDownloadProgress {
                    // A throw here would panic the core, as in [render].
                    override fun onProgress(received: ULong, total: ULong) {
                        runCatching { report(received, total) }
                    }
                }
            },
        )

    /** Tell the node that the entries [openHopChatResource] gave were taken, once the bytes are kept. */
    @Throws(HostRejection::class)
    suspend fun ackHopChatResource(productId: String, resource: ChatResource, entries: List<ByteArray>) =
        inner.ackHopChatResource(productId, resource, entries)

    /**
     * Tell the pairing host behind [deeplink] that allowance allocation is
     * under way, so it leaves its QR screen while the allocation runs.
     *
     * Answering needs this host's own statement-store allowance, so register
     * the `WalletSso` renewal target first. The peer's own device statement
     * account is the other target, read with `parsePairingDeeplink` and
     * tracked before [establishPairing] runs; the allocation this notice
     * covers is what that call waits on. The returned handle is owed a
     * [notifyPairingFailed] if pairing then fails: the peer has dropped its QR
     * and waits without a deadline of its own.
     *
     * The handle holds the responder statement secret the notice was signed
     * with, and nothing consumes it, so `destroy()` it once the pairing
     * settles — on the succeeding path as well as the failing one. Wrapping
     * the whole pairing in `use { }` covers both.
     */
    suspend fun notifyPairingAllowanceAllocation(deeplink: String): NativeAnnouncedPairing =
        inner.notifyPairingAllowanceAllocation(deeplink)

    /**
     * Tell a pairing host that already dropped its QR why pairing stopped.
     *
     * Takes the handle from [notifyPairingAllowanceAllocation], so the notice
     * is signed by the account that already reached that peer even if this
     * host's signer has rotated since.
     */
    suspend fun notifyPairingFailed(announced: NativeAnnouncedPairing, reason: String) {
        inner.notifyPairingFailed(announced, reason)
    }

    /**
     * Answer a pairing host's handshake deeplink, without serving the session
     * it opens.
     *
     * The answer is signed by this host's own SSO statement identity, so the
     * `WalletSso` renewal target has to be allocated for it to reach the
     * Statement Store at all. The peer's device statement account is the other
     * tracked target, since this host allocates the allowance the peer authors
     * its own session statements under; read it from the deeplink with
     * `parsePairingDeeplink`. A pairing that fails after that leaves the peer's
     * target to untrack again, unless the device was already paired and the
     * target still carries a live pairing.
     *
     * A device that pairs here reaches [HostBridge.devicePaired]. Serving the
     * session is [resumePairing], called with the peer this host persisted.
     */
    suspend fun establishPairing(deeplink: String) {
        inner.establishPairing(deeplink)
    }

    /**
     * Serve a paired host's SSO session until it ends.
     *
     * Runs for the life of the session, so give it its own coroutine. Only
     * [ResponderExit.PEER_DISCONNECTED] authorises dropping the stored
     * pairing; after [ResponderExit.SUBSCRIPTION_ENDED] or a thrown error the
     * peer is still paired and this can be called again.
     */
    suspend fun resumePairing(peer: PairedSsoPeer): ResponderExit = inner.resumePairing(peer)

    /**
     * Tell a paired host this signing host is ending their SSO session.
     *
     * Submits the disconnect notice and nothing else. The local side is the
     * caller's: cancel that peer's [resumePairing] coroutine, which otherwise
     * keeps answering a host this one no longer considers paired, and untrack
     * its device statement account, which otherwise keeps being renewed every
     * period. Dropping the stored pairing alone leaves both running.
     */
    suspend fun disconnectPairedHost(peer: PairedSsoPeer) {
        inner.disconnectPairedHost(peer)
    }

    /** Core-owned logout for the process-wide authentication session. */
    fun disconnect() {
        inner.disconnect()
    }

    /** Report the core database's SQLite version, schema version and file path. */
    @Throws(NativeCoreDatabaseException::class)
    suspend fun coreDatabaseStatus(): DbStatus = inner.coreDatabaseStatus()

    /**
     * Decide durable transactions from the chain until none is live, then
     * return. Await it from the background task [HostBridge.durableWorkChanged]
     * asks for; a throw means recovery stopped early and the task should be
     * retried. Cancelling the coroutine stops it.
     */
    @Throws(RecoveryException::class)
    suspend fun runDurableRecovery() = inner.runDurableRecovery()

    /** Activate or replace the process-wide local signing session. */
    @Throws(HostRejection::class)
    fun activateLocalSession(secret: ByteArray, liteUsername: String? = null) {
        inner.activateLocalSession(secret, liteUsername)
    }

    /** Push a JSON-RPC response from a native chain connection into the runtime. */
    fun notifyChainResponse(connectionId: UInt, json: String) {
        inner.notifyChainResponse(connectionId, json)
    }

    /** Notify the runtime that a native chain connection closed externally. */
    fun notifyChainClosed(connectionId: UInt) {
        inner.notifyChainClosed(connectionId)
    }

    /**
     * Record the accounts renewal should keep allowed on the Statement Store.
     * Needs an active session, so call it after [activateLocalSession] or after
     * pairing, not at construction.
     *
     * Recipe-shaped targets survive a change of root entropy; a raw
     * [StatementRenewalTarget.Account] does not, so re-track those
     * whenever the active identity changes.
     */
    @Throws(HostRejection::class)
    fun trackStatementRenewalTargets(targets: List<StatementRenewalTarget>) {
        inner.trackStatementRenewalTargets(targets)
    }

    /**
     * The accounts the ledger tracks, in the order they were tracked. Needs no
     * active session, so a worker can read it on a cold start before deciding
     * whether a pass is worth running.
     */
    @Throws(HostRejection::class)
    fun statementRenewalTargets(): List<TrackedStatementRenewalTarget> =
        inner.statementRenewalTargets()

    /**
     * The root public key the active identity records its fixed entries under.
     * An entry from [statementRenewalTargets] whose owner is this key, or which
     * has no owner, is one a pass will renew; any other is one it will prune.
     */
    @Throws(HostRejection::class)
    fun statementRenewalOwnerKey(): ByteArray = inner.statementRenewalOwnerKey()

    /**
     * Stop renewing one fixed statement account, reporting whether the ledger
     * held it. Scoped to the active identity, so it never removes an entry
     * another identity promised.
     */
    @Throws(HostRejection::class)
    fun untrackStatementRenewalAccount(accountId: ByteArray): Boolean =
        inner.untrackStatementRenewalAccount(accountId)

    /**
     * Run one renewal pass now, reporting what each tracked target got. Submits
     * extrinsics and blocks until they are included, so call it from a
     * WorkManager worker rather than the main thread.
     */
    @Throws(HostRejection::class)
    fun renewStatementAllowances(): StatementRenewalReport = inner.renewStatementAllowances()

    /**
     * Start the in-process renewal loop, for a host that stays resident. A
     * suspended app stops ticking, so prefer scheduling
     * [renewStatementAllowances].
     */
    fun startStatementAllowanceRenewal() {
        inner.startStatementAllowanceRenewal()
    }

    /**
     * The in-process loop's own cadence, capped at an hour. A host scheduling
     * one wake-up per period should read a value under an hour as the boundary
     * approaching rather than waking hourly.
     */
    fun nextStatementRenewalDelay(): java.time.Duration = inner.nextStatementRenewalDelay()

    override fun close() {
        inner.close()
    }
}

/**
 * How long the host waits for the answer to a chat query. After the timeout
 * the host shows its failure state for that query.
 */
object ChatQueryTimeout {
    /** A page of messages, or of the messages that match a search. */
    val MESSAGES: Duration = 10.seconds

    /** A message or a payment that the user sent. */
    val SENT_MESSAGE: Duration = 30.seconds

    /** A user action that sends nothing. */
    val USER_ACTION: Duration = 10.seconds

    /** The edit history of a message. */
    val LOOKUP: Duration = 5.seconds

    /** The room a link opens: the product may first do network work, such as sending a chat request. */
    val OPEN_LINK: Duration = 90.seconds

    /** The timeout of one user action. */
    fun forUserAction(action: ChatUserAction): Duration =
        when (action) {
            is ChatUserAction.NewMessage, is ChatUserAction.SendPayment -> SENT_MESSAGE
            else -> USER_ACTION
        }
}

/** Why a chat query has no answer. */
sealed class ChatQueryException(message: String) : Exception(message) {
    /**
     * The product, or the core for a room that does not declare the handler,
     * refused the query. [ProductChatQueryError.NotAvailable] asks the host to
     * apply its default.
     */
    class Refused(val error: ProductChatQueryError) : ChatQueryException("chat query refused: $error")

    /**
     * The product, or the core for a room that the host stores, refused the
     * user action. The host reverts the action, and shows the reason of
     * [ProductChatUserActionError.Rejected] to the user.
     */
    class ActionRefused(val error: ProductChatUserActionError) :
        ChatQueryException("chat user action refused: $error")

    /** The query failed on its way, for example because the worker stopped. */
    class Failed(val reason: String) : ChatQueryException(reason)

    /** The product ended the query without an answer. */
    class NoAnswer : ChatQueryException("the product ended the chat query without an answer")

    /** No answer came before the timeout. */
    class TimedOut(val timeout: Duration) : ChatQueryException("no answer to the chat query in $timeout")
}

/**
 * The first result of one chat query: the answer of the product or its
 * refusal. Later results have no effect.
 */
internal class ChatQueryOutcome : NativeChatQueryObserver {
    val answer = CompletableDeferred<NativeChatQueryAnswer>()

    // The core declares all three infallible, so uniffi has no error type to
    // convert a throw into and panics, which aborts under `panic = "abort"`.
    override fun onAnswer(answer: NativeChatQueryAnswer) {
        runCatching { this.answer.complete(answer) }
    }

    override fun onRefusal(refusal: NativeChatQueryRefusal) {
        val error =
            when (refusal) {
                is NativeChatQueryRefusal.Query -> ChatQueryException.Refused(refusal.error)
                is NativeChatQueryRefusal.UserAction -> ChatQueryException.ActionRefused(refusal.error)
                is NativeChatQueryRefusal.Failed -> ChatQueryException.Failed(refusal.reason)
            }
        runCatching { answer.completeExceptionally(error) }
    }

    override fun onComplete() {
        runCatching { answer.completeExceptionally(ChatQueryException.NoAnswer()) }
    }
}

/**
 * Start one chat query, and wait for its answer. A timeout or a cancelled
 * coroutine stops the query in the product.
 */
internal suspend fun <T> chatQuery(
    timeout: Duration,
    start: (NativeChatQueryObserver) -> NativeChatQuery,
    extract: (NativeChatQueryAnswer) -> T?,
): T {
    val outcome = ChatQueryOutcome()
    val query = start(outcome)
    try {
        val answer = withTimeoutOrNull(timeout) { outcome.answer.await() }
            ?: throw ChatQueryException.TimedOut(timeout)
        return extract(answer) ?: throw ChatQueryException.Failed("the product answered another query")
    } finally {
        query.cancel()
        query.close()
    }
}

/** A render the product declined or could not encode. */
class RendererStreamException(
    /** Why the product ended the render. */
    val reason: String,
) : Exception(reason)

/**
 * One SPA or Chat executable connected to a shared [TrUAPIHostRuntime]. Closing
 * it shuts the connection down permanently; the runtime stays usable.
 */
class TrUAPIProductExecution internal constructor(
    private val inner: NativeProductExecution,
    private val callbackRetainer: HostCallbacks,
    private val chatRetainer: NativeChatCallbacks?,
    private val pocketRetainer: NativePocketCallbacks?,
    private val gameRetainer: NativeGameCallbacks?,
) : AutoCloseable {
    private val shutDown = AtomicBoolean(false)

    /**
     * Register this execution against the host runtime's shared localhost
     * bridge, minting an independent authentication token. Every execution
     * under the same host runtime connects through the same port.
     */
    @Throws(WsBridgeStartException::class)
    fun startWsBridge(bindPort: UShort = 0u): WsBridgeEndpoint = inner.startWsBridge(bindPort)

    /** Revoke this execution's bridge registration while leaving it reusable. */
    fun stopWsBridge() {
        inner.stopWsBridge()
    }

    /**
     * Publish one native Chat action, buffering it until the product
     * connection subscribes.
     */
    @Throws(ProductRuntimeException::class)
    fun publishChatAction(action: HostChatActionSubscribeItem) {
        inner.publishChatAction(action)
    }

    /** Ask the product for one page of messages of a room that it keeps. */
    @Throws(ProductRuntimeException::class, ChatQueryException::class)
    suspend fun chatMessagesQuery(
        request: ProductChatMessagesQueryRequest,
        timeout: Duration = ChatQueryTimeout.MESSAGES,
    ): ProductChatMessagesQueryItem =
        chatQuery(timeout, { inner.chatMessagesQuery(request, it) }) {
            (it as? NativeChatQueryAnswer.Messages)?.page
        }

    /** Tell the product what the user did in a room that it keeps. */
    @Throws(ProductRuntimeException::class, ChatQueryException::class)
    suspend fun chatUserAction(
        request: ProductChatUserActionRequest,
        timeout: Duration = ChatQueryTimeout.forUserAction(request.action),
    ): ProductChatUserActionItem =
        chatQuery(timeout, { inner.chatUserAction(request, it) }) {
            (it as? NativeChatQueryAnswer.UserAction)?.answer
        }

    /**
     * Ask the product for the messages of a room that it keeps that match what
     * the user typed, newest first.
     */
    @Throws(ProductRuntimeException::class, ChatQueryException::class)
    suspend fun chatSearchQuery(
        request: ProductChatSearchQueryRequest,
        timeout: Duration = ChatQueryTimeout.MESSAGES,
    ): ProductChatSearchQueryItem =
        chatQuery(timeout, { inner.chatSearchQuery(request, it) }) {
            (it as? NativeChatQueryAnswer.Search)?.page
        }

    /** Ask the product for the texts one message had, oldest first. */
    @Throws(ProductRuntimeException::class, ChatQueryException::class)
    suspend fun chatEditHistory(
        request: ProductChatEditHistoryQueryRequest,
        timeout: Duration = ChatQueryTimeout.LOOKUP,
    ): ProductChatEditHistoryQueryItem =
        chatQuery(timeout, { inner.chatEditHistory(request, it) }) {
            (it as? NativeChatQueryAnswer.EditHistory)?.history
        }

    /**
     * Give the product something the user opened from an entry point of the
     * host, such as a contact link or a username, and get the room to show.
     */
    suspend fun chatOpenLink(
        request: ProductChatOpenLinkRequest,
        timeout: Duration = ChatQueryTimeout.OPEN_LINK,
    ): ProductChatOpenLinkItem =
        chatQuery(timeout, { inner.chatOpenLink(request, it) }) {
            (it as? NativeChatQueryAnswer.OpenLink)?.answer
        }

    /**
     * Republish the product-scoped native Chat room list. Call it whenever the
     * host's own rooms change, including when a host joins a registered bot to
     * a room.
     */
    fun notifyChatRoomsChanged(rooms: List<ChatRoom>) {
        inner.notifyChatRoomsChanged(rooms)
    }

    /**
     * Request a native renderer tree for one render context. The flow
     * subscribes on collection, so a closed or non-Worker execution fails the
     * collector with [ProductRuntimeException] rather than this call. It
     * cancels the renderer when collection ends;
     * each emission is a complete replacement tree, so only the latest is kept
     * when the collector falls behind.
     */
    fun render(request: ProductRendererRenderRequest): Flow<RendererNode> =
        callbackFlow {
            val observer =
                object : NativeRendererObserver {
                    // The core declares all three infallible, so uniffi has no
                    // error type to convert a throw into and panics -- which
                    // aborts under `panic = "abort"`.
                    override fun onUpdate(node: RendererNode) {
                        runCatching { trySend(node) }
                    }

                    override fun onComplete() {
                        runCatching { close() }
                    }

                    // The last tree sent is partial, so closing with a cause
                    // keeps this distinct from a clean end for the collector.
                    override fun onError(reason: String) {
                        runCatching { close(RendererStreamException(reason)) }
                    }
                }
            val subscription = inner.render(request, observer)
            awaitClose {
                subscription.cancel()
                subscription.close()
            }
        }.conflate()

    /**
     * Publish one native renderer action, buffering it until the product
     * connection subscribes.
     */
    @Throws(ProductRuntimeException::class)
    fun publishRendererAction(item: HostRendererActionSubscribeItem) {
        inner.publishRendererAction(item)
    }

    /**
     * Republish the product-scoped card list. Call it whenever the host's own
     * collection changes, including after the user removes a card.
     */
    fun notifyPocketCardsChanged(cards: List<PocketCard>) {
        inner.notifyPocketCardsChanged(cards)
    }

    /** Read the active session's X25519 chat identity private key, if any. */
    @Throws(HostRejection::class)
    fun sessionChatIdentityKey(): ByteArray? = inner.sessionChatIdentityKey()

    /**
     * Read a permission authorization status without prompting.
     *
     * A device capability resolves the OS gate as well as storage, so an OS
     * refusal reads as `Denied` whatever is stored. Remote, identity disclosure
     * and account access decisions have no OS gate.
     */
    @Throws(HostRejection::class)
    suspend fun permissionAuthorizationStatus(
        request: PermissionAuthorizationRequest,
    ): PermissionAuthorizationStatus = inner.permissionAuthorizationStatus(request)

    /** Authorize one native network operation, consuming an existing Allow once grant. */
    @Throws(HostRejection::class)
    suspend fun authorizeRemotePermission(request: RemotePermissionRequest): Boolean =
        inner.authorizeRemotePermission(request)

    /**
     * Authorize one device capability the web view gates itself, using the
     * product's saved, one-use, or prompted decision.
     */
    @Throws(HostRejection::class)
    suspend fun authorizeDevicePermission(request: HostDevicePermissionRequest): Boolean =
        inner.authorizeDevicePermission(request)

    /**
     * Update a stored permission authorization status. Passing `NotDetermined`
     * clears the stored value so the next product request prompts again.
     */
    @Throws(HostRejection::class)
    fun setPermissionAuthorizationStatus(
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) {
        inner.setPermissionAuthorizationStatus(request, status)
    }

    /** Push a host theme update to active TrUAPI theme subscriptions. */
    fun notifyThemeChanged(theme: HostThemeSubscribeItem) {
        inner.notifyThemeChanged(theme)
    }

    /** Push a host locale update to active TrUAPI locale subscriptions. */
    fun notifyLocaleChanged(locale: HostLocaleSubscribeItem) {
        inner.notifyLocaleChanged(locale)
    }

    /**
     * Push a host storage change to active TrUAPI storage subscriptions, across
     * every execution of the product; a null [value] means cleared.
     *
     * Only for changes the host makes itself. A write a product made through
     * TrUAPI already reaches its subscribers, so reporting one here delivers it
     * twice.
     */
    fun notifyStorageChanged(key: String, value: ByteArray?) {
        inner.notifyStorageChanged(key, value)
    }

    /** Push a preimage lookup update to active subscriptions for [key]. */
    fun notifyPreimageChanged(key: ByteArray, value: ByteArray?) {
        inner.notifyPreimageChanged(key, value)
    }

    /** Push a JSON-RPC response from a native chain connection into the core. */
    fun notifyChainResponse(connectionId: UInt, json: String) {
        inner.notifyChainResponse(connectionId, json)
    }

    /** Notify the core that a native chain connection closed externally. */
    fun notifyChainClosed(connectionId: UInt) {
        inner.notifyChainClosed(connectionId)
    }

    @Synchronized
    override fun close() {
        // `shutdown` goes through the generated call guard, which throws once
        // the handle is freed, so a repeat close must not reach it. Serialized
        // as well as guarded: a concurrent close could otherwise free the
        // handle between the guard and the call.
        if (shutDown.compareAndSet(false, true)) {
            inner.shutdown()
        }
        inner.close()
    }
}
