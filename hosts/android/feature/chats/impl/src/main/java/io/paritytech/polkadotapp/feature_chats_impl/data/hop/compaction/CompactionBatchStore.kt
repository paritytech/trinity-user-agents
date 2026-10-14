package io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopPoolEntryPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.VersionedHopPoolEntry
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryableTransferException
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TerminalTransferException
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray
import javax.inject.Inject

class CompactionBatchStore @Inject constructor(
    private val hopService: HopService,
    private val hopSigner: HopSigner,
    private val hopNodeUrlProvider: HopNodeUrlProvider
) {
    val batchBudgetBytes: Int = HopService.INLINE_MAX_BYTES

    /**
     * Greedily splits [rawMessages] into batches so that each batch, SCALE-encoded
     * as a `List<ByteArray>`, fits into [batchBudgetBytes]. A message that cannot fit
     * even a dedicated batch fails with [IllegalArgumentException].
     */
    fun packBatches(rawMessages: List<ByteArray>): List<List<ByteArray>> {
        val batches = mutableListOf<List<ByteArray>>()
        var currentBatch = ScaleBatchBuilder()

        for (message in rawMessages) {
            require(ScaleBatchBuilder.dedicatedBatchSize(message) <= batchBudgetBytes) {
                "Message of ${message.size} bytes cannot fit even a dedicated compaction batch " +
                    "of $batchBudgetBytes bytes"
            }

            if (!currentBatch.isEmpty() && currentBatch.sizeWith(message) > batchBudgetBytes) {
                batches += currentBatch.messages
                currentBatch = ScaleBatchBuilder()
            }

            currentBatch.add(message)
        }

        if (!currentBatch.isEmpty()) {
            batches += currentBatch.messages
        }

        return batches
    }

    suspend fun uploadBatch(rawMessages: List<ByteArray>, ticket: HopTicket, nodeUrl: String): DataByteArray {
        val batch = BinaryScale.encodeToByteArray(rawMessages)
        require(batch.size <= batchBudgetBytes) {
            "Compacted batch of ${batch.size} bytes exceeds the upload budget of $batchBudgetBytes bytes"
        }

        val envelope: VersionedHopPoolEntry = VersionedHopPoolEntry.V1(HopPoolEntryPayload.Inline(batch))

        hopSigner.ensureCanSubmit()

        return hopService.withSession(nodeUrl) {
            submitEntry(BinaryScale.encodeToByteArray(envelope), ticket).toDataByteArray()
        }
    }

    suspend fun claimBatch(
        identifier: DataByteArray,
        ticket: HopTicket,
        nodeUrl: String,
        persist: suspend (ByteArray) -> Unit
    ) {
        hopService.withSession(nodeUrl) {
            val fallbackNodes = hopNodeUrlProvider.allWithSenderPriority(nodeUrl)

            val entry = fetchEntry(identifier.value, ticket, fallbackNodes)
                ?: throw RetryableTransferException(
                    "Compacted batch ${identifier.value.toHexString(withPrefix = true)} " +
                        "was not found in the pool or on any fallback node"
                )

            persist(extractInlineBatch(entry.bytes, identifier))

            entry.markPersisted()
        }
    }

    fun decodeBatchMessages(batchBytes: ByteArray): List<ByteArray> {
        return BinaryScale.decodeFromByteArray(batchBytes)
    }

    private fun extractInlineBatch(entryBytes: ByteArray, identifier: DataByteArray): ByteArray {
        val batch = when (val payload = VersionedHopPoolEntry.decodePayload(entryBytes)) {
            is HopPoolEntryPayload.Inline -> payload.bytes
            is HopPoolEntryPayload.Chunked -> throw TerminalTransferException(
                "Compacted batch ${identifier.value.toHexString(withPrefix = true)} " +
                    "uses a chunked payload, which is malformed for compaction"
            )
        }

        if (batch.size > batchBudgetBytes) {
            throw TerminalTransferException(
                "Compacted batch of ${batch.size} bytes exceeds the allowed maximum of $batchBudgetBytes bytes"
            )
        }

        return batch
    }
}

private class ScaleBatchBuilder {
    private var elementsSize = 0L

    val messages: List<ByteArray>
        field = mutableListOf<ByteArray>()

    fun isEmpty(): Boolean = messages.isEmpty()

    fun sizeWith(message: ByteArray): Long {
        return compactPrefixSize(messages.size + 1) + elementsSize + elementSize(message)
    }

    fun add(message: ByteArray) {
        elementsSize += elementSize(message)
        messages += message
    }

    companion object {
        fun dedicatedBatchSize(message: ByteArray): Long = compactPrefixSize(1) + elementSize(message)

        private fun elementSize(message: ByteArray): Long = compactPrefixSize(message.size) + message.size

        private fun compactPrefixSize(value: Int): Long = when {
            value < 64 -> 1L
            value < 16_384 -> 2L
            value < 1_073_741_824 -> 4L
            else -> 5L
        }
    }
}
