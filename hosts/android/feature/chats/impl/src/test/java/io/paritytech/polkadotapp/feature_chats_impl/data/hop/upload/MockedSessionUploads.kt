package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.mockk.verify
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.ChatMessageAttachmentUpdater
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopChunkedPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopPoolEntryPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.VersionedHopPoolEntry
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.AttachmentFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import kotlinx.coroutines.flow.flowOf
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals

const val VIDEO_MP4 = "video/mp4"
const val LARGE_IMAGE_SIZE = 3_000_000

private const val HASH_SIZE = 32

val STORED_HASH_0 = ByteArray(HASH_SIZE) { 0x70 }.toHexString(withPrefix = true)
val STORED_HASH_1 = ByteArray(HASH_SIZE) { 0x71 }.toHexString(withPrefix = true)
val STORED_HASH_2 = ByteArray(HASH_SIZE) { 0x72 }.toHexString(withPrefix = true)

class MockedSessionUploads {
    val fileUri: Uri = mockk()

    private val attachmentFileStorage: AttachmentFileStorage = mockk()
    private val hopService: HopService = mockk()
    private val session: HopService.Session = mockk()
    private val hopSigner: HopSigner = mockk()
    private val fileUploadRepository: FileUploadRepository = mockk(relaxUnitFun = true)
    private val messageAttachmentUpdater: ChatMessageAttachmentUpdater = mockk(relaxUnitFun = true)
    private val attachmentMetaBuilder: AttachmentMetaBuilder = mockk()
    private val preProcessor: FileUploadPreProcessor = mockk()

    private val submittedPlaintexts = mutableListOf<ByteArray>()
    private val returnedHashes = mutableListOf<ByteArray>()

    private val uploader = HopFileUploader(
        attachmentFileStorage = attachmentFileStorage,
        hopService = hopService,
        hopSigner = hopSigner,
        fileUploadRepository = fileUploadRepository,
        messageAttachmentUpdater = messageAttachmentUpdater,
        attachmentMetaBuilder = attachmentMetaBuilder,
        preProcessors = setOf(preProcessor)
    )

    val submitCount: Int get() = submittedPlaintexts.size

    fun returnedHash(index: Int): ByteArray = returnedHashes[index]

    suspend fun upload(upload: FileUpload) = uploader.upload(upload)

    suspend fun withUploadAllowed() {
        withSubmitAllowed(hopSigner)
        withMarkingImagePreProcessor(preProcessor)
    }

    suspend fun withSuccessfulSession() {
        coEvery { hopService.withSession<Any?>(any(), any()) } coAnswers {
            secondArg<suspend HopService.Session.() -> Any?>()(session)
        }
        coEvery { session.submitEntry(any(), any()) } answers {
            submittedPlaintexts += firstArg<ByteArray>()
            val hash = ByteArray(HASH_SIZE) { submittedPlaintexts.size.toByte() }
            returnedHashes += hash
            hash
        }
        withAttachmentMeta(attachmentMetaBuilder)
    }

    suspend fun withFileBytes(size: Int): ByteArray = withFileBytes(attachmentFileStorage, fileUri, size)

    suspend fun withFileSize(size: Int) {
        coEvery { attachmentFileStorage.sizeOf(fileUri) } returns size.bytes
    }

    fun withVideoChunks(fromChunk: Int, slices: List<ByteArray>) {
        every { attachmentFileStorage.readChunks(fileUri, fromChunk) } returns flowOf(*slices.toTypedArray())
    }

    suspend fun withUploadStillActive() = withUploadStillActive(fileUploadRepository)

    suspend fun verifySessionOpenedOn(nodeUrl: String) {
        coVerify(exactly = 1) { hopService.withSession<Any?>(nodeUrl, any()) }
    }

    fun verifyStreamedFrom(chunk: Int) {
        verify(exactly = 1) { attachmentFileStorage.readChunks(fileUri, chunk) }
    }

    fun verifyNeverStreamed() {
        verify(exactly = 0) { attachmentFileStorage.readChunks(any(), any()) }
    }

    suspend fun verifyReadWhole() {
        coVerify(exactly = 1) { attachmentFileStorage.readFileBytes(fileUri) }
    }

    suspend fun verifyNeverReadWhole() {
        coVerify(exactly = 0) { attachmentFileStorage.readFileBytes(any()) }
    }

    suspend fun verifyNeverSized() {
        coVerify(exactly = 0) { attachmentFileStorage.sizeOf(any()) }
    }

    fun verifyNeverPreProcessed() {
        verify(exactly = 0) { preProcessor.preProcess(any()) }
    }

    suspend fun verifyNoProgressRecorded() {
        coVerify(exactly = 0) { fileUploadRepository.updateProgress(any(), any(), any()) }
    }

    suspend fun firstRecordedProgress(): Int {
        val uploadedChunks = mutableListOf<Int>()
        coVerify(atLeast = 1) { fileUploadRepository.updateProgress(any(), capture(uploadedChunks), any()) }
        return uploadedChunks.first()
    }

    suspend fun sentAttachment(): Attachment.Hosted {
        val attachment = slot<Attachment.Hosted>()
        coVerify(exactly = 1) { messageAttachmentUpdater.updateSentAttachment(any(), any(), capture(attachment)) }
        return attachment.captured
    }

    fun chunkPlaintexts(): List<ByteArray> = submittedPlaintexts.dropLast(1)

    fun assertSubmittedChunks(expected: List<ByteArray>) {
        expected.forEachIndexed { index, slice ->
            assertArrayEquals("chunk submit $index", slice, submittedPlaintexts[index])
        }
    }

    fun assertRootListsHashes(expected: List<ByteArray>) {
        val root = decodeChunkedRoot(submittedPlaintexts.last())
        assertEquals(expected.size, root.chunks.size)
        expected.zip(root.chunks).forEachIndexed { index, (expectedHash, listedHash) ->
            assertArrayEquals("listed hash $index", expectedHash, listedHash)
        }
    }

    private fun decodeChunkedRoot(plaintext: ByteArray): HopChunkedPayload {
        val envelope = BinaryScale.decodeFromByteArray<VersionedHopPoolEntry>(plaintext)
        val payload = (envelope as VersionedHopPoolEntry.V1).payload
        return (payload as HopPoolEntryPayload.Chunked).payload
    }
}
