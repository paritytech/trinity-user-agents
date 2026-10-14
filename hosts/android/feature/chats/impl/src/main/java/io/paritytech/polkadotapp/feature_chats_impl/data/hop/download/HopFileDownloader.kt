package io.paritytech.polkadotapp.feature_chats_impl.data.hop.download

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.ChatMessageAttachmentUpdater
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopPoolEntryPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.VersionedHopPoolEntry
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryableTransferException
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferCancelledException
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileDownloadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.AttachmentFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileDownload
import java.io.File
import java.io.FileOutputStream
import javax.inject.Inject

class HopFileDownloader @Inject constructor(
    private val hopService: HopService,
    private val hopNodeUrlProvider: HopNodeUrlProvider,
    private val attachmentFileStorage: AttachmentFileStorage,
    private val fileDownloadRepository: FileDownloadRepository,
    private val messageAttachmentUpdater: ChatMessageAttachmentUpdater,
    private val fileProvider: FileProvider
) {
    suspend fun download(download: FileDownload) {
        check(hopNodeUrlProvider.isAllowed(download.nodeUrl)) {
            "Node url ${download.nodeUrl} is not in the known Hop node allowlist — refusing to download"
        }

        when (val payload = download.progress.payload) {
            is FileDownload.Payload.Unresolved -> downloadFromRoot(download)
            is FileDownload.Payload.Chunked -> resumeChunked(download, payload)
            is FileDownload.Payload.Inline -> finalizeInline(download)
        }
    }

    private suspend fun downloadFromRoot(download: FileDownload) {
        val rotationNode = pickRotationNode(download)

        hopService.withSession(download.nodeUrl) {
            val root = fetchEntryOrRetry(download.identifier.value, download.ticket, rotationNode)

            when (val entryPayload = VersionedHopPoolEntry.decodePayload(root.bytes)) {
                is HopPoolEntryPayload.Inline -> completeInline(download, entryPayload, root)
                is HopPoolEntryPayload.Chunked -> downloadChunked(download, entryPayload, root, rotationNode)
            }
        }
    }

    private suspend fun completeInline(
        download: FileDownload,
        payload: HopPoolEntryPayload.Inline,
        root: HopService.FetchedEntry
    ) {
        val file = resolveFile(download)
        file.writeBytes(payload.bytes)

        fileDownloadRepository.resolveInline(download.messageId, file.absolutePath)
        updateMessage(download, file)
        root.markPersisted()
    }

    private suspend fun HopService.Session.downloadChunked(
        download: FileDownload,
        payload: HopPoolEntryPayload.Chunked,
        root: HopService.FetchedEntry,
        rotationNode: String
    ) {
        val chunkHashes = payload.payload.chunks

        fileDownloadRepository.resolveChunked(
            messageId = download.messageId,
            chunkHashes = chunkHashes.map { it.toHexString(withPrefix = true) }
        )
        root.markPersisted()

        val file = resolveFile(download)

        downloadChunks(
            download = download,
            file = file,
            chunkHashes = chunkHashes,
            startFrom = 0,
            rotationNode = rotationNode
        )

        updateMessage(download, file)
    }

    private suspend fun resumeChunked(download: FileDownload, payload: FileDownload.Payload.Chunked) {
        val rotationNode = pickRotationNode(download)

        hopService.withSession(download.nodeUrl) {
            val file = resolveFile(download)
            downloadChunks(
                download = download,
                file = file,
                chunkHashes = payload.chunkHashes.map { it.fromHex() },
                startFrom = payload.downloadedChunks,
                rotationNode = rotationNode
            )
            updateMessage(download, file)
        }
    }

    private suspend fun finalizeInline(download: FileDownload) {
        updateMessage(download, File(checkNotNull(download.filePath)))
    }

    private fun resolveFile(download: FileDownload): File {
        download.filePath?.let { return File(it) }

        return attachmentFileStorage.createDownloadFile(download.mimeType)
    }

    private suspend fun HopService.Session.downloadChunks(
        download: FileDownload,
        file: File,
        chunkHashes: List<ByteArray>,
        startFrom: Int,
        rotationNode: String
    ) {
        val remaining = chunkHashes.drop(startFrom)
        val appendFile = startFrom > 0

        FileOutputStream(file, appendFile).use { output ->
            for ((index, chunkHash) in remaining.withIndex()) {
                if (!fileDownloadRepository.isActive(download.messageId)) throw TransferCancelledException()

                val fetched = fetchEntryOrRetry(chunkHash, download.ticket, rotationNode)

                output.write(fetched.bytes)
                output.fd.sync()

                fileDownloadRepository.updateProgress(
                    messageId = download.messageId,
                    downloadedChunks = startFrom + index + 1,
                    filePath = file.absolutePath
                )

                fetched.markPersisted()
            }
        }
    }

    private suspend fun HopService.Session.fetchEntryOrRetry(
        hash: ByteArray,
        ticket: HopTicket,
        rotationNode: String
    ): HopService.FetchedEntry {
        return fetchEntry(hash, ticket, fallbackNodes = listOf(rotationNode))
            ?: throw RetryableTransferException(
                "Entry ${hash.toHexString(withPrefix = true)} not available from pool or chain yet"
            )
    }

    private suspend fun pickRotationNode(download: FileDownload): String {
        val ordered = hopNodeUrlProvider.allWithSenderPriority(download.nodeUrl)
        return ordered[download.progress.retryState.attemptCount % ordered.size]
    }

    private suspend fun updateMessage(download: FileDownload, file: File) {
        messageAttachmentUpdater.updateReceivedAttachment(
            chatId = download.chatId,
            messageId = download.messageId,
            identifier = download.identifier,
            uri = fileProvider.uriOf(file)
        )
    }
}
