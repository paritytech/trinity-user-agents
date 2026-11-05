package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.net.Uri
import io.mockk.Runs
import io.mockk.coEvery
import io.mockk.every
import io.mockk.just
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.NODE_A
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.patternBytes
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.AttachmentFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.HopTransferRetryState
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

const val MESSAGE_ID = "message-a"
const val IMAGE_JPEG = "image/jpeg"
const val INLINE_FILE_SIZE = 1024
val FIRST_BACKOFF = 10.seconds
const val MARKER_BYTE: Byte = 0x7F

private const val FILE_NAME = "file"
private const val IMAGE_MIME_PREFIX = "image/"
private const val TICKET_SIZE = 32
private const val CHAT_ID_SIZE = 32
private val UPLOAD_TICKET = HopTicket.fromRaw(ByteArray(TICKET_SIZE) { it.toByte() })
private val UPLOAD_CHAT_ID = ChatId.fromRawValue(ByteArray(CHAT_ID_SIZE) { 1 })

fun fileUpload(
    uri: Uri,
    messageId: String = MESSAGE_ID,
    mimeType: String = IMAGE_JPEG,
    nodeUrl: String = NODE_A,
    attemptCount: Int = 0,
    uploadedChunkHashes: List<String> = emptyList()
) = FileUpload(
    messageId = messageId,
    chatId = UPLOAD_CHAT_ID,
    ticket = UPLOAD_TICKET,
    nodeUrl = nodeUrl,
    size = FileUpload.Size.NotMeasured,
    meta = FileUpload.Meta(uri = uri, mimeType = mimeType),
    progress = FileUpload.Progress(
        status = FileUpload.Status.PENDING,
        totalChunks = 0,
        uploadedChunks = uploadedChunkHashes.size,
        uploadedChunkHashes = uploadedChunkHashes,
        error = null,
        retryState = retryStateAfter(attemptCount)
    ),
    createdAt = 0L
)

fun withMarkingImagePreProcessor(preProcessor: FileUploadPreProcessor, onPreProcess: () -> Unit = {}) {
    every { preProcessor.appliesTo(any()) } answers { firstArg<String>().startsWith(IMAGE_MIME_PREFIX) }
    every { preProcessor.preProcess(any()) } answers {
        onPreProcess()
        firstArg<ByteArray>() + MARKER_BYTE
    }
}

suspend fun withSubmitAllowed(hopSigner: HopSigner) {
    coEvery { hopSigner.ensureCanSubmit() } just Runs
}

fun withUploadSlots(uploadConcurrency: UploadConcurrency, count: Int) {
    every { uploadConcurrency.maxInFlight() } returns count
}

suspend fun withFileBytes(storage: AttachmentFileStorage, uri: Uri, size: Int): ByteArray {
    val bytes = patternBytes(size)
    coEvery { storage.readFileBytes(uri) } returns bytes
    return bytes
}

suspend fun withAttachmentMeta(attachmentMetaBuilder: AttachmentMetaBuilder) {
    coEvery { attachmentMetaBuilder.build(any(), any(), any()) } returns
        Attachment.Meta.General(FILE_NAME, IMAGE_JPEG, InformationSize.ZERO)
}

suspend fun withUploadStillActive(repository: FileUploadRepository) {
    coEvery { repository.isActive(any()) } returns true
}

private fun retryStateAfter(attemptCount: Int): HopTransferRetryState {
    if (attemptCount == 0) return HopTransferRetryState.None

    return HopTransferRetryState(
        attemptCount = attemptCount,
        firstFailureAt = Instant.fromEpochMilliseconds(0),
        nextAttemptAt = Instant.fromEpochMilliseconds(0) + FIRST_BACKOFF
    )
}
