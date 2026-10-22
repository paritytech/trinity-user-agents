package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatHopFile
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatHopFiles
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import kotlinx.coroutines.CancellationException
import javax.inject.Inject

/**
 * Opens the HOP files of the chat product through `NativeTrUApiHostRuntime::open_hop_chat_resource`. The core claims
 * them from the nodes [AppPersonChatHostBridge] lists and checks every entry; this class opens no connection.
 */
class TrUAPIChatHopFiles @Inject constructor(
    // Lazy: the runtime provider reaches the worker supervisor, which reaches the chat extensions.
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    private val dotNsTldProvider: DotNsTldProvider,
) : ChatHopFiles {
    override suspend fun open(resource: ChatResourceModel.Hop, progress: ((received: Long, total: Long) -> Unit)?): Result<ChatHopFile> =
        call { runtime, productId ->
            val report = progress?.let { { received: ULong, total: ULong -> it(received.toLong(), total.toLong()) } }
            val file = runtime.openHopChatResource(productId, resource.toCore(), report)
            ChatHopFile(file.bytes, file.entries)
        }

    override suspend fun ack(resource: ChatResourceModel.Hop, entries: List<ByteArray>): Result<Unit> =
        call { runtime, productId -> runtime.ackHopChatResource(productId, resource.toCore(), entries) }

    private suspend fun <T> call(block: suspend (TrUAPIHostRuntime, String) -> T): Result<T> {
        val runtime = runtimeProvider.get().runtime().getOrElse { return Result.failure(it) }
        val tld = dotNsTldProvider.getTld().getOrElse { return Result.failure(it) }
        return try {
            Result.success(block(runtime, ReservedProductIds.chat(tld).value))
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Result.failure(error)
        }
    }
}
