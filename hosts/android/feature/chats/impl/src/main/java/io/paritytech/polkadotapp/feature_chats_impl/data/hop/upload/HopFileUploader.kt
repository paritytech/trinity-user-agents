package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.net.Uri
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.chunkedSequence
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.ChatMessageAttachmentUpdater
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopChunkedPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopPoolEntryPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.VersionedHopPoolEntry
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryableTransferException
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferCancelledException
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.AttachmentFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.asFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.encodeToByteArray
import javax.inject.Inject
import kotlin.math.ceil
import kotlin.time.Duration.Companion.seconds

class HopFileUploader @Inject constructor(
    private val attachmentFileStorage: AttachmentFileStorage,
    private val hopService: HopService,
    private val hopSigner: HopSigner,
    private val fileUploadRepository: FileUploadRepository,
    private val messageAttachmentUpdater: ChatMessageAttachmentUpdater,
    private val attachmentMetaBuilder: AttachmentMetaBuilder,
    private val preProcessors: Set<@JvmSuppressWildcards FileUploadPreProcessor>
) {
    private val preProcessingLock = Mutex()

    suspend fun upload(upload: FileUpload) {
        hopSigner.ensureCanSubmit()

        val content = readContent(upload.meta)

        hopService.withSession(upload.nodeUrl) {
            val identifier = if (content is UploadContent.InMemory && content.size.inWholeBytes <= HopService.INLINE_MAX_BYTES) {
                fileUploadRepository.updateFileInfo(upload.messageId, content.size, totalChunks = 1)
                submitInlineRoot(content.bytes, upload.ticket)
            } else {
                val totalChunks = determineNumberOfChunks(content.size)
                fileUploadRepository.updateFileInfo(upload.messageId, content.size, totalChunks)
                val chunkHashes = uploadChunks(upload, content, totalChunks)
                submitChunkedRoot(content.size.inWholeBytes.toULong(), chunkHashes, upload.ticket)
            }

            val attachmentMeta = attachmentMetaBuilder.build(
                uri = upload.meta.uri,
                mimeType = upload.meta.mimeType,
                size = content.size
            )

            updateMessage(upload, identifier, attachmentMeta)
        }
    }

    private suspend fun readContent(meta: FileUpload.Meta): UploadContent {
        val applicable = preProcessors.filter { it.appliesTo(meta.mimeType) }

        if (applicable.isNotEmpty()) {
            return preProcessingLock.withLock {
                val raw = attachmentFileStorage.readFileBytes(meta.uri)
                UploadContent.InMemory(applicable.fold(raw) { bytes, processor -> processor.preProcess(bytes) })
            }
        }

        val size = attachmentFileStorage.sizeOf(meta.uri)
        if (size.inWholeBytes <= HopService.INLINE_MAX_BYTES) {
            return UploadContent.InMemory(attachmentFileStorage.readFileBytes(meta.uri))
        }

        return UploadContent.OnDisk(meta.uri, size)
    }

    private suspend fun HopService.Session.submitInlineRoot(payload: ByteArray, ticket: HopTicket): ByteArray {
        val envelope: VersionedHopPoolEntry = VersionedHopPoolEntry.V1(HopPoolEntryPayload.Inline(payload))
        return submitWithinTimeout(BinaryScale.encodeToByteArray(envelope), ticket)
    }

    private suspend fun HopService.Session.submitChunkedRoot(
        totalSize: ULong,
        chunkHashes: List<ByteArray>,
        ticket: HopTicket
    ): ByteArray {
        val envelope: VersionedHopPoolEntry = VersionedHopPoolEntry.V1(
            HopPoolEntryPayload.Chunked(HopChunkedPayload(totalSize = totalSize, chunks = chunkHashes))
        )
        return submitWithinTimeout(BinaryScale.encodeToByteArray(envelope), ticket)
    }

    private suspend fun HopService.Session.uploadChunks(
        upload: FileUpload,
        content: UploadContent,
        totalChunks: Int
    ): List<ByteArray> {
        val allChunkHashes = upload.progress.uploadedChunkHashes.map { it.fromHex() }.toMutableList()

        if (allChunkHashes.size >= totalChunks) return allChunkHashes

        content.chunks(fromChunk = allChunkHashes.size).collect { chunk ->
            if (!fileUploadRepository.isActive(upload.messageId)) throw TransferCancelledException()

            allChunkHashes.add(submitWithinTimeout(chunk, upload.ticket))

            fileUploadRepository.updateProgress(
                messageId = upload.messageId,
                uploadedChunks = allChunkHashes.size,
                chunkHashes = allChunkHashes.map { it.toHexString(withPrefix = true) }
            )
        }

        return allChunkHashes
    }

    private suspend fun HopService.Session.submitWithinTimeout(payload: ByteArray, ticket: HopTicket): ByteArray {
        return withTimeoutOrNull(HOP_REQUEST_TIMEOUT) { submitEntry(payload, ticket) }
            ?: throw RetryableTransferException("Hop submit did not complete within $HOP_REQUEST_TIMEOUT")
    }

    private fun UploadContent.chunks(fromChunk: Int): Flow<ByteArray> = when (this) {
        is UploadContent.OnDisk -> attachmentFileStorage.readChunks(uri, fromChunk)
        is UploadContent.InMemory -> bytes.chunkedSequence(HopService.CHUNK_SIZE_BYTES).drop(fromChunk).asFlow()
    }

    private suspend fun updateMessage(upload: FileUpload, metadataHash: ByteArray, meta: Attachment.Meta) {
        val attachment = Attachment.Hosted(
            uri = upload.meta.uri,
            identifier = metadataHash.toDataByteArray(),
            ticket = upload.ticket,
            nodeUrl = upload.nodeUrl,
            meta = meta
        )

        messageAttachmentUpdater.updateSentAttachment(
            chatId = upload.chatId,
            messageId = upload.messageId,
            attachment = attachment
        )
    }

    private sealed interface UploadContent {
        val size: InformationSize

        class InMemory(val bytes: ByteArray) : UploadContent {
            override val size: InformationSize = bytes.size.toLong().bytes
        }

        class OnDisk(val uri: Uri, override val size: InformationSize) : UploadContent
    }

    companion object {
        private val HOP_REQUEST_TIMEOUT = 60.seconds

        fun determineNumberOfChunks(fileSize: InformationSize): Int {
            return ceil(fileSize.inWholeBytes.toDouble() / HopService.CHUNK_SIZE_BYTES).toInt()
        }
    }
}
