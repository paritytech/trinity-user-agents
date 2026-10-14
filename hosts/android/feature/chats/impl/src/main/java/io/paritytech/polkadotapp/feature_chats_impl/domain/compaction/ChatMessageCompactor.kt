package io.paritytech.polkadotapp.feature_chats_impl.domain.compaction

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction.CompactionBatchStore
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatement
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatementContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NodeEndpoint
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.VersionedChatMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CompactedBatch
import io.paritytech.polkadotapp.feature_statement_store_api.domain.MessageCompactor
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import kotlinx.coroutines.delay
import kotlinx.serialization.encodeToByteArray
import timber.log.Timber
import java.util.UUID
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

class ChatMessageCompactor @Inject constructor(
    private val compactionBatchStore: CompactionBatchStore,
    private val hopNodeUrlProvider: HopNodeUrlProvider
) : MessageCompactor {
    override suspend fun compact(messages: List<EncodedMessage>): Result<List<CompactedBatch>> {
        return runCatching { compactionBatchStore.packBatches(messages) }
            .flatMap { chunks -> uploadChunks(chunks) }
    }

    private suspend fun uploadChunks(chunks: List<List<EncodedMessage>>): Result<List<CompactedBatch>> {
        val batches = mutableListOf<CompactedBatch>()
        var retriesLeft = UPLOAD_RETRIES

        for (chunk in chunks) {
            var uploaded = false

            while (!uploaded) {
                runCancellableCatching { uploadChunk(chunk) }
                    .onSuccess { batch ->
                        batches += batch
                        uploaded = true
                    }
                    .onFailure { error ->
                        if (error is IllegalArgumentException || retriesLeft == 0) {
                            return Result.failure(error)
                        }

                        retriesLeft--
                        Timber.w(error, "Compaction batch upload failed, $retriesLeft retries left")
                        delay(RETRY_DELAY)
                    }
            }
        }

        return Result.success(batches)
    }

    private suspend fun uploadChunk(chunk: List<EncodedMessage>): CompactedBatch {
        val nodeUrl = hopNodeUrlProvider.pickForSending()
        val ticket = HopTicket.random()
        val identifier = compactionBatchStore.uploadBatch(chunk, ticket, nodeUrl)

        return CompactedBatch(
            commit = encodeCommitStatement(identifier.value, ticket, nodeUrl),
            originals = chunk
        )
    }

    private fun encodeCommitStatement(identifier: ByteArray, ticket: HopTicket, nodeUrl: String): EncodedMessage {
        val content = ChatMessageStatementContent.CompactionCommit(
            claimIdentifier = identifier,
            claimTicket = ticket.bytes,
            node = NodeEndpoint.WssUrl(nodeUrl)
        )

        val statement = ChatMessageStatement(
            id = UUID.randomUUID().toString(),
            timestamp = System.currentTimeMillis().toULong(),
            versioned = VersionedChatMessage.V1(ChatMessageV1(content))
        )

        return BinaryScale.encodeToByteArray(statement)
    }

    companion object {
        private const val UPLOAD_RETRIES = 3
        private val RETRY_DELAY = 2.seconds
    }
}
