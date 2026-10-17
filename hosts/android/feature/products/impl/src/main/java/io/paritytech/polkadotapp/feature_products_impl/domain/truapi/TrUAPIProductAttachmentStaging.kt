package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductAttachmentStageException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductAttachmentStaging
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.productRoomId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.WorkerExecutionState
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import uniffi.truapi.NativeChatStageRejection
import javax.inject.Inject

/**
 * Stages the attachments of product rooms through `NativeTrUApiHostRuntime::chat_stage_attachment` (07-LATER §7).
 * The core reads the room declaration, asks for `PreimageSubmit` and pays from the Bulletin allowance of the product.
 * This class holds no key and opens no connection.
 */
class TrUAPIProductAttachmentStaging @Inject constructor(
    // Lazy: the runtime provider reaches the worker supervisor, which reaches the chat extensions.
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    private val workers: Lazy<TrUAPIWorkerSupervisor>,
) : ProductAttachmentStaging {
    override suspend fun <T> holdingWorker(chatId: ChatId, block: suspend () -> T): T {
        val (productId, _) = chatId.productRoom() ?: return block()
        val runtime = runtimeProvider.get().runtime().getOrElse { return block() }

        val held = runCatching { runtime.acquireWorker(productId.value) }
            .onFailure { Timber.w(it, "truapi.chat.stage cannot hold the worker of %s", productId.value) }
            .isSuccess
        try {
            // When the worker does not run, the core reads only the declaration that it saw, and cannot ask the user.
            val running = withTimeoutOrNull(TrUAPIWorkerSupervisor.EXECUTION_WAIT_TIMEOUT) {
                workers.get().executionState(productId).first { it is WorkerExecutionState.Running }
            }
            if (running == null) Timber.w("truapi.chat.stage: the worker of %s does not run", productId.value)
            return block()
        } finally {
            if (held) {
                withContext(NonCancellable) {
                    runCatching { runtime.releaseWorker(productId.value) }
                        .onFailure { Timber.w(it, "truapi.chat.stage cannot release the worker of %s", productId.value) }
                }
            }
        }
    }

    override suspend fun stage(chatId: ChatId, mimeType: String, bytes: ByteArray): Result<DataByteArray> {
        val (productId, room) = chatId.productRoom()
            ?: return Result.failure(ProductAttachmentStageException.NotDeclared())
        val runtime = runtimeProvider.get().runtime()
            .getOrElse { return Result.failure(ProductAttachmentStageException.SubmitFailed(it.message ?: it.toString())) }

        return try {
            Result.success(runtime.stageChatAttachment(productId.value, room.value, mimeType, bytes).toDataByteArray())
        } catch (rejection: NativeChatStageRejection) {
            Result.failure(rejection.toModel())
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Result.failure(ProductAttachmentStageException.SubmitFailed(error.message ?: error.toString()))
        }
    }

    override suspend fun seal(chatId: ChatId, bytes: ByteArray): Result<ChatResourceModel.Sealed> {
        val (productId, room) = chatId.productRoom()
            ?: return Result.failure(ProductAttachmentStageException.NotDeclared())
        val runtime = runtimeProvider.get().runtime()
            .getOrElse { return Result.failure(ProductAttachmentStageException.SubmitFailed(it.message ?: it.toString())) }

        return try {
            when (val resource = runtime.sealChatAttachment(productId.value, room.value, bytes).toModel()) {
                is ChatResourceModel.Sealed -> Result.success(resource)
                else -> Result.failure(ProductAttachmentStageException.SubmitFailed("the core gave no sealed resource"))
            }
        } catch (rejection: NativeChatStageRejection) {
            Result.failure(rejection.toModel())
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Result.failure(ProductAttachmentStageException.SubmitFailed(error.message ?: error.toString()))
        }
    }

    private fun ChatId.productRoom(): Pair<ProductId, ProductChatIdParameter>? {
        val extensionId = extensionOrNull()?.extensionId ?: return null
        val productId = ProductId.fromStoredValue(extensionId.removePrefix(PRODUCT_EXTENSION_PREFIX))
        if (productId.toChatExtensionId() != extensionId) return null
        return productRoomId(extensionId)?.let { productId to it }
    }

    private companion object {
        const val PRODUCT_EXTENSION_PREFIX = "ProductBot_"
    }
}

fun NativeChatStageRejection.toModel(): ProductAttachmentStageException = when (this) {
    is NativeChatStageRejection.NotDeclared -> ProductAttachmentStageException.NotDeclared()
    is NativeChatStageRejection.TooLarge -> ProductAttachmentStageException.TooLarge(maxBytes.toLong())
    is NativeChatStageRejection.PermissionDenied -> ProductAttachmentStageException.PermissionDenied()
    is NativeChatStageRejection.AllowanceUnavailable -> ProductAttachmentStageException.AllowanceUnavailable(reason)
    is NativeChatStageRejection.SubmitFailed -> ProductAttachmentStageException.SubmitFailed(reason)
}
