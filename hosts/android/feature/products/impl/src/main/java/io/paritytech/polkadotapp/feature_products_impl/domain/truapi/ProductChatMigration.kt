package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.SharedPreferences
import dagger.Lazy
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatMapNotNull
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatHistoryExporter
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatPaymentAdoption
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import timber.log.Timber
import uniffi.truapi.HostHistoryExportChunk
import java.io.File
import java.io.RandomAccessFile

/**
 * Moves this install's native chat to the chat product once, while the native chat's tables are still there (HX3):
 * stages its history for the product's import and gives the core the native device key, so contacts keep reaching
 * this device. Runs at launch; the product's first `historyExport` waits for it. The staged file sits in the app's
 * no-backup storage, as private as the native chat's own database.
 *
 * Then, once, it hands the native chat's payment messages over to the host's records of the chat product's payments
 * ([NativeChatPaymentAdoption]), so their states show in the product's rooms and an unclaimed incoming payment is still
 * claimed. It has its own flag, so an install that staged its history before this step existed takes its payments too.
 */
class ProductChatMigration(
    private val exporter: NativeChatHistoryExporter,
    private val payments: NativeChatPaymentAdoption,
    private val tldProvider: DotNsTldProvider,
    private val deviceKeys: OurDeviceKeypairProvider,
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    private val preferences: SharedPreferences,
    private val file: File,
) : AppInitializer {
    private val staged = CompletableDeferred<Unit>()

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        scope.launch {
            stage().logFailure("The native chat history was not staged")
            staged.complete(Unit)
            adoptPayments().logFailure("The native chat payments were not taken over")
        }
        return Result.success(Unit)
    }

    /** At most [maxLen] bytes from [offset] of the staged history, and its size; a size of 0 when none is staged. */
    suspend fun part(offset: ULong, maxLen: UInt): HostHistoryExportChunk {
        staged.await()
        if (!file.exists()) return HostHistoryExportChunk(total = 0u, data = ByteArray(0))
        return RandomAccessFile(file, "r").use { history ->
            val total = history.length()
            val start = offset.toLong().coerceIn(0, total)
            val data = ByteArray(minOf(maxLen.toLong(), total - start).toInt())
            history.seek(start)
            history.readFully(data)
            HostHistoryExportChunk(total = total.toULong(), data = data)
        }
    }

    fun imported() {
        file.delete()
    }

    private suspend fun stage(): Result<Unit> {
        if (preferences.getBoolean(KEY_STAGED, false)) return Result.success(Unit)
        return runCancellableCatching { exporter.export() }
            .flatMapNotNull { history -> keep(history) }
            .map { preferences.edit().putBoolean(KEY_STAGED, true).apply() }
    }

    private suspend fun keep(history: ByteArray): Result<Unit> = runCancellableCatching {
        val partial = File(file.parentFile, "${file.name}.partial")
        partial.writeBytes(history)
        check(partial.renameTo(file)) { "the staged history could not be moved into place" }
    }
        .flatMap { runtimeProvider.get().runtime() }
        .flatMap { runtime -> runCancellableCatching { runtime.importChatDeviceKey(deviceKeys.get().privateKey.bytes.value) } }
        .map { kept ->
            if (!kept) Timber.w("The core keeps another chat device key; contacts of the native chat may not reach this device")
        }

    private suspend fun adoptPayments(): Result<Unit> {
        if (preferences.getBoolean(KEY_PAYMENTS_ADOPTED, false)) return Result.success(Unit)
        return runCancellableCatching {
            // The rooms of the contacts are the chat product's of this network: `dm-<account hex>`.
            val chat = ReservedProductIds.chat(tldProvider.getTldRetrying()).toChatExtensionId()
            payments.adopt { account -> ProductChatIdParameter(contactRoomId(account)).toChatId(chat) }
        }.map { preferences.edit().putBoolean(KEY_PAYMENTS_ADOPTED, true).apply() }
    }

    private companion object {
        const val KEY_STAGED = "staged"
        const val KEY_PAYMENTS_ADOPTED = "payments_adopted"
    }
}

/** The room of the chat product with a contact (`contactRoomId` of `packages/product/src/surface/mapping.ts`). */
internal fun contactRoomId(account: AccountId): String = "dm-${account.value.toHexString(withPrefix = false)}"
