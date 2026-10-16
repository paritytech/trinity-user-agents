package io.paritytech.polkadotapp.app.debug.e2e

import android.content.BroadcastReceiver
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
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.e2e.E2EAcks
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.e2e.E2ERuntimeMarkers
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.e2e.E2E_LOG_TAG
import io.paritytech.polkadotapp.feature_products_impl.domain.productBotManagement.ProductBotManagementInteractor
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import timber.log.Timber

/**
 * Debug-only entry point the chat e2e drives: seeds an identity, registers a product, and posts a
 * user message through the real chat path.
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
    }

    override fun onReceive(context: Context, intent: Intent) {
        val hooks = EntryPointAccessors.fromApplication(context.applicationContext, Hooks::class.java)
        hooks.e2eRuntimeMarkers().enabled = true

        val pendingResult = goAsync()

        scope.launch {
            try {
                handle(hooks, intent)
            } finally {
                pendingResult.finish()
            }
        }
    }

    private suspend fun handle(hooks: Hooks, intent: Intent) {
        if (intent.getBooleanExtra(EXTRA_SEED_IDENTITY, false)) {
            seedIdentity(hooks, username = intent.stringExtra(EXTRA_USERNAME) ?: DEFAULT_USERNAME)
        }

        val productId = intent.stringExtra(EXTRA_PRODUCT_ID)?.let(ProductId::fromStoredValue)
        val message = intent.stringExtra(EXTRA_MESSAGE)

        val workerUrl = intent.stringExtra(EXTRA_WORKER_URL)
        if (productId != null && workerUrl != null) {
            registerProduct(hooks, productId, workerUrl, intent.stringExtra(EXTRA_PRODUCT_NAME) ?: productId.value)
        }

        if (message != null) {
            val roomId = intent.stringExtra(EXTRA_ROOM_ID)
            when {
                productId == null -> logError("message", "message requires $EXTRA_PRODUCT_ID")
                roomId == null -> logError("message", "message requires $EXTRA_ROOM_ID")
                else -> sendUserMessage(hooks, productId, roomId, message)
            }
        }
    }

    private suspend fun sendUserMessage(hooks: Hooks, productId: ProductId, roomId: String, text: String) {
        val chatId = ChatId.forExtensionRoom(productId.toChatExtensionId(), roomId)
        runCatching { hooks.chatMessageSender().sendUserMessage(chatId = chatId, content = ChatMessage.Content.Text(text)) }
            .fold(
                onSuccess = { log(E2EAcks.messageSent(productId.value, roomId)) },
                onFailure = { logError("message", it) },
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

        const val DEFAULT_USERNAME = "truapi-e2e"

        val E2E_ENTROPY = ByteArray(16) { 0x42 }

        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    }
}
