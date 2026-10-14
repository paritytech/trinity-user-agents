package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.chunked
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.ChatMessageAttachmentUpdater
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopChunkedPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopPoolEntryPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.VersionedHopPoolEntry
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferCancelledException
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.AttachmentFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import kotlinx.serialization.encodeToByteArray
import javax.inject.Inject
import kotlin.math.ceil

class HopFileUploader @Inject constructor(
    private val attachmentFileStorage: AttachmentFileStorage,
    private val hopService: HopService,
    private val hopSigner: HopSigner,
    private val fileUploadRepository: FileUploadRepository,
    private val messageAttachmentUpdater: ChatMessageAttachmentUpdater,
    private val attachmentMetaBuilder: AttachmentMetaBuilder,
    private val preProcessors: Set<@JvmSuppressWildcards FileUploadPreProcessor>
) {
    suspend fun upload(upload: FileUpload) {
        hopSigner.ensureCanSubmit()

        hopService.withSession(upload.nodeUrl) {
            val raw = attachmentFileStorage.readFileBytes(upload.meta.uri)
            val fileBytes = preProcessors.fold(raw) { bytes, processor -> processor.preProcess(bytes, upload.meta.mimeType) }

            val fileSize = fileBytes.size.toLong().bytes

            val identifier = if (fileBytes.size <= HopService.INLINE_MAX_BYTES) {
                fileUploadRepository.updateFileInfo(upload.messageId, fileSize, totalChunks = 1)
                submitInlineRoot(fileBytes, upload.ticket)
            } else {
                fileUploadRepository.updateFileInfo(upload.messageId, fileSize, determineNumberOfChunks(fileSize))
                val chunkHashes = uploadChunks(upload, fileBytes)
                submitChunkedRoot(fileBytes.size.toULong(), chunkHashes, upload.ticket)
            }

            val attachmentMeta = attachmentMetaBuilder.build(
                uri = upload.meta.uri,
                mimeType = upload.meta.mimeType,
                size = fileSize
            )

            updateMessage(upload, identifier, attachmentMeta)
        }
    }

    private suspend fun HopService.Session.submitInlineRoot(payload: ByteArray, ticket: HopTicket): ByteArray {
        val envelope: VersionedHopPoolEntry = VersionedHopPoolEntry.V1(HopPoolEntryPayload.Inline(payload))
        return submitEntry(BinaryScale.encodeToByteArray(envelope), ticket)
    }

    private suspend fun HopService.Session.submitChunkedRoot(
        totalSize: ULong,
        chunkHashes: List<ByteArray>,
        ticket: HopTicket
    ): ByteArray {
        val envelope: VersionedHopPoolEntry = VersionedHopPoolEntry.V1(
            HopPoolEntryPayload.Chunked(HopChunkedPayload(totalSize = totalSize, chunks = chunkHashes))
        )
        return submitEntry(BinaryScale.encodeToByteArray(envelope), ticket)
    }

    private suspend fun HopService.Session.uploadChunks(
        upload: FileUpload,
        fileBytes: ByteArray
    ): List<ByteArray> {
        val previousHashes = upload.progress.uploadedChunkHashes.map { it.fromHex() }
        val allChunkHashes = previousHashes.toMutableList()

        val chunks = fileBytes.chunked(HopService.CHUNK_SIZE_BYTES)

        for (i in upload.progress.uploadedChunks until chunks.size) {
            if (!fileUploadRepository.isActive(upload.messageId)) throw TransferCancelledException()

            val chunkHash = submitEntry(chunks[i], upload.ticket)
            allChunkHashes.add(chunkHash)

            fileUploadRepository.updateProgress(
                messageId = upload.messageId,
                uploadedChunks = i + 1,
                chunkHashes = allChunkHashes.map { it.toHexString(withPrefix = true) }
            )
        }

        return allChunkHashes
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

    companion object {
        fun determineNumberOfChunks(fileSize: InformationSize): Int {
            return ceil(fileSize.inWholeBytes.toDouble() / HopService.CHUNK_SIZE_BYTES).toInt()
        }
    }
}
