package io.paritytech.polkadotapp.app.debug.e2e

import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_backup_impl.ManualMnemonicInteractor
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStagingGate
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.e2e.E2EAcks
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.e2e.E2ERuntimeMarkers
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.e2e.E2E_LOG_TAG
import io.paritytech.polkadotapp.feature_products_impl.domain.productBotManagement.ProductBotManagementInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import timber.log.Timber
import uniffi.truapi.ChatWakeOutcome
import uniffi.truapi.ChatWakeReason
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.RemotePermission
import uniffi.truapi.RemotePermissionRequest

/**
 * Debug-only entry point the chat e2e drives: seeds an identity, registers a product, posts a
 * user message through the real chat path, puts a text on the clipboard, and wakes a product for a room.
 *
 * With the clipboard hook, the launcher can type a text that `adb shell input text` cannot type,
 * for example an emoji. The launcher sets the text here, and then pastes it into the input.
 *
 * TODO: seeding state through a broadcast is implicit. The explicit alternative is swapping parts
 * of the DI graph for test implementations, such as an account repository that answers fixtures.
 */
class E2EHookReceiver : BroadcastReceiver() {

    @EntryPoint
    @InstallIn(SingletonComponent::class)
    interface Hooks {
        fun accountRepository(): AccountRepository
        fun manualMnemonicInteractor(): ManualMnemonicInteractor
        fun localUsernameStorage(): LocalUsernameStorage
        fun productBotManagementInteractor(): ProductBotManagementInteractor
        fun chatMessageSender(): ChatMessageSender
        fun e2eRuntimeMarkers(): E2ERuntimeMarkers
        fun trUAPIWorkerSupervisor(): TrUAPIWorkerSupervisor
        fun trUAPIHostRuntimeProvider(): TrUAPIHostRuntimeProvider
        fun productAttachmentStagingGate(): ProductAttachmentStagingGate
    }

    override fun onReceive(context: Context, intent: Intent) {
        val hooks = EntryPointAccessors.fromApplication(context.applicationContext, Hooks::class.java)
        hooks.e2eRuntimeMarkers().enabled = true

        val pendingResult = goAsync()

        scope.launch {
            try {
                handle(context, hooks, intent)
            } finally {
                pendingResult.finish()
            }
        }
    }

    private suspend fun handle(context: Context, hooks: Hooks, intent: Intent) {
        intent.getStringExtra(EXTRA_CLIPBOARD)?.let { setClipboard(context, it) }

        if (intent.getBooleanExtra(EXTRA_SEED_IDENTITY, false)) {
            seedIdentity(hooks, username = intent.stringExtra(EXTRA_USERNAME) ?: DEFAULT_USERNAME)
        }

        val productId = intent.stringExtra(EXTRA_PRODUCT_ID)?.let(ProductId::fromStoredValue)
        val message = intent.stringExtra(EXTRA_MESSAGE)

        val workerUrl = intent.stringExtra(EXTRA_WORKER_URL)
        if (productId != null && workerUrl != null) {
            registerProduct(hooks, productId, workerUrl, intent.stringExtra(EXTRA_PRODUCT_NAME) ?: productId.value)
        }

        if (intent.getBooleanExtra(EXTRA_SERVE_PREIMAGE_ATTACHMENTS, false)) {
            hooks.productAttachmentStagingGate().serveForE2E()
            log(SERVE_PREIMAGE_ATTACHMENTS_ACK)
        }

        if (intent.getBooleanExtra(EXTRA_RESET_PREIMAGE_PERMISSION, false)) {
            if (productId == null) logError("preimage_permission", "requires $EXTRA_PRODUCT_ID") else resetPreimagePermission(hooks, productId)
        }

        val probeBytes = intent.getIntExtra(EXTRA_STAGE_PROBE, 0)
        if (probeBytes > 0) {
            if (productId == null) logError("stage_probe", "requires $EXTRA_PRODUCT_ID") else probeStaging(hooks, productId, probeBytes)
        }

        if (message != null) {
            val roomId = intent.stringExtra(EXTRA_ROOM_ID)
            when {
                productId == null -> logError("message", "message requires $EXTRA_PRODUCT_ID")
                roomId == null -> logError("message", "message requires $EXTRA_ROOM_ID")
                else -> sendUserMessage(hooks, productId, roomId, message)
            }
        }

        intent.stringExtra(EXTRA_WAKE)?.let { watchId ->
            val roomId = intent.stringExtra(EXTRA_ROOM_ID)
            when {
                productId == null -> logError("wake", "wake requires $EXTRA_PRODUCT_ID")
                roomId == null -> logError("wake", "wake requires $EXTRA_ROOM_ID")
                else -> wake(hooks, productId, roomId, watchId)
            }
        }
    }

    // B9: a notification wake of a room, as the receiving layer will start it (07-LATER §10.2). The receiving layer
    // is not on this branch yet (O28), so the hook takes its place.
    private suspend fun wake(hooks: Hooks, productId: ProductId, roomId: String, watchId: String) {
        hooks.trUAPIHostRuntimeProvider().runtime()
            .mapCatching { it.wakeProduct(productId.value, ChatWakeReason.Notification(roomId, watchId)) }
            .fold(
                onSuccess = { outcome ->
                    val text = when (outcome) {
                        is ChatWakeOutcome.Held -> "held until=${outcome.untilMs}"
                        ChatWakeOutcome.BudgetSpent -> "budget-spent"
                    }
                    log(E2EAcks.woken(productId.value, roomId, watchId, text))
                },
                onFailure = { logError("wake", it) },
            )
    }

    private suspend fun sendUserMessage(hooks: Hooks, productId: ProductId, roomId: String, text: String) {
        val chatId = ChatId.forExtensionRoom(productId.toChatExtensionId(), roomId)
        runCatching { hooks.chatMessageSender().sendUserMessage(chatId = chatId, content = ChatMessage.Content.Text(text)) }
            .fold(
                onSuccess = { log(E2EAcks.messageSent(productId.value, roomId)) },
                onFailure = { logError("message", it) },
            )
    }

    // A stored "Don't allow" of the first start would refuse every staged attachment of the run. This is the reset
    // that a user makes in the settings of the product. The worker of the product must run.
    private fun resetPreimagePermission(hooks: Hooks, productId: ProductId) {
        val execution = hooks.trUAPIWorkerSupervisor().currentExecution(productId)
            ?: return logError("preimage_permission", "the worker of ${productId.value} does not run")
        runCatching {
            execution.setPermissionAuthorizationStatus(
                PermissionAuthorizationRequest.Remote(RemotePermissionRequest(RemotePermission.PreimageSubmit)),
                PermissionAuthorizationStatus.NOT_DETERMINED,
            )
        }.fold(
            onSuccess = { log(PREIMAGE_PERMISSION_RESET_ACK) },
            onFailure = { logError("preimage_permission", it) },
        )
    }

    // B7: the time that the bytes of an attachment take to cross the binding into the core. The probe room declares
    // nothing, so the core answers `NotDeclared` after the bytes arrive, and uploads nothing.
    private suspend fun probeStaging(hooks: Hooks, productId: ProductId, size: Int) {
        val runtime = hooks.trUAPIHostRuntimeProvider().runtime().getOrElse { return logError("stage_probe", it) }
        val bytes = ByteArray(size) { (it % PROBE_BYTE_MODULUS).toByte() }
        val timings = mutableListOf<Long>()
        var outcome = "Staged"
        repeat(PROBE_ROUNDS) {
            val started = System.nanoTime()
            outcome = runCatching { runtime.stageChatAttachment(productId.value, PROBE_ROOM, PROBE_MIME_TYPE, bytes) }
                .fold(onSuccess = { "Staged" }, onFailure = { it::class.simpleName ?: "Failure" })
            timings += (System.nanoTime() - started) / NANOS_IN_MILLI
        }
        log("stage_probe bytes=$size ms=${timings.joinToString(",")} outcome=$outcome")
    }

    private suspend fun setClipboard(context: Context, text: String) {
        runCatching {
            withContext(Dispatchers.Main) {
                val clipboard = context.getSystemService(ClipboardManager::class.java)
                clipboard.setPrimaryClip(ClipData.newPlainText(CLIP_LABEL, text))
            }
        }.fold(
            onSuccess = { log(CLIPBOARD_SET_ACK) },
            onFailure = { logError("clipboard", it) },
        )
    }

    private suspend fun seedIdentity(hooks: Hooks, username: String) {
        runCatching {
            if (!hooks.accountRepository().areAccountsInitialized()) {
                hooks.manualMnemonicInteractor().createAccounts(E2E_ENTROPY).getOrThrow()
            }
            hooks.localUsernameStorage().saveValue(Username.fromFullValue(username))
        }.fold(
            onSuccess = { log(E2EAcks.SEED_IDENTITY_DONE) },
            onFailure = { logError("seed_identity", it) }
        )
    }

    private suspend fun registerProduct(hooks: Hooks, productId: ProductId, workerUrl: String, name: String) {
        hooks.productBotManagementInteractor().upsertProduct(productId, workerUrl, name, card = null, appUrl = null).fold(
            onSuccess = { log(E2EAcks.productRegistered(productId.value)) },
            onFailure = { logError("product", it) }
        )
    }

    private fun Intent.stringExtra(name: String): String? = getStringExtra(name)?.takeIf { it.isNotBlank() }

    private fun log(line: String) = Timber.tag(E2E_LOG_TAG).i(line)

    private fun logError(hook: String, error: Throwable) =
        logError(hook, error.message ?: error::class.java.simpleName)

    private fun logError(hook: String, reason: String) = Timber.tag(E2E_LOG_TAG).i(E2EAcks.error(hook, reason))

    private companion object {
        const val EXTRA_SEED_IDENTITY = "seed_identity"
        const val EXTRA_USERNAME = "username"
        const val EXTRA_PRODUCT_ID = "product_id"
        const val EXTRA_PRODUCT_NAME = "product_name"
        const val EXTRA_WORKER_URL = "worker_url"
        const val EXTRA_MESSAGE = "message"
        const val EXTRA_ROOM_ID = "room_id"
        const val EXTRA_CLIPBOARD = "clipboard"
        const val EXTRA_WAKE = "wake"
        const val EXTRA_RESET_PREIMAGE_PERMISSION = "reset_preimage_permission"
        const val EXTRA_STAGE_PROBE = "stage_probe"
        const val EXTRA_SERVE_PREIMAGE_ATTACHMENTS = "serve_preimage_attachments"

        const val PREIMAGE_PERMISSION_RESET_ACK = "preimage permission reset"
        const val SERVE_PREIMAGE_ATTACHMENTS_ACK = "preimage attachments served"
        const val PROBE_ROOM = "b7-probe"
        const val PROBE_MIME_TYPE = "application/octet-stream"
        const val PROBE_ROUNDS = 7
        const val PROBE_BYTE_MODULUS = 251
        const val NANOS_IN_MILLI = 1_000_000L

        const val CLIP_LABEL = "truapi-e2e"
        const val CLIPBOARD_SET_ACK = "clipboard set"

        const val DEFAULT_USERNAME = "truapi-e2e"

        val E2E_ENTROPY = ByteArray(16) { 0x42 }

        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    }
}
