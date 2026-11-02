package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.net.Uri
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.ChatMessageAttachmentUpdater
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.anyChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction.anyHopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopChunkedPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopPoolEntryPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.VersionedHopPoolEntry
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.AttachmentFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.anyInt
import io.paritytech.polkadotapp.test_shared.capture
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.mockito.ArgumentCaptor
import org.mockito.Mockito.atLeastOnce
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

const val VIDEO_MP4 = "video/mp4"
const val LARGE_IMAGE_SIZE = 3_000_000

private const val HASH_SIZE = 32

val STORED_HASH_0 = ByteArray(HASH_SIZE) { 0x70 }.toHexString(withPrefix = true)
val STORED_HASH_1 = ByteArray(HASH_SIZE) { 0x71 }.toHexString(withPrefix = true)
val STORED_HASH_2 = ByteArray(HASH_SIZE) { 0x72 }.toHexString(withPrefix = true)

class MockedSessionUploads {
    val fileUri: Uri = mock(Uri::class.java)

    private val attachmentFileStorage: AttachmentFileStorage = mock(AttachmentFileStorage::class.java)
    private val hopService: HopService = mock(HopService::class.java)
    private val session: HopService.Session = mock(HopService.Session::class.java)
    private val hopSigner: HopSigner = mock(HopSigner::class.java)
    private val fileUploadRepository: FileUploadRepository = mock(FileUploadRepository::class.java)
    private val messageAttachmentUpdater: ChatMessageAttachmentUpdater = mock(ChatMessageAttachmentUpdater::class.java)
    private val attachmentMetaBuilder: AttachmentMetaBuilder = mock(AttachmentMetaBuilder::class.java)
    private val preProcessor: FileUploadPreProcessor = mock(FileUploadPreProcessor::class.java)

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
        whenever(hopService.withSession<Any?>(any(), any())).thenAnswer { invocation ->
            @Suppress("UNCHECKED_CAST")
            val block = invocation.arguments[1] as suspend HopService.Session.() -> Any?
            runBlocking { block(session) }
        }
        whenever(session.submitEntry(any(), anyHopTicket())).thenAnswer { invocation ->
            submittedPlaintexts += invocation.arguments[0] as ByteArray
            val hash = ByteArray(HASH_SIZE) { submittedPlaintexts.size.toByte() }
            returnedHashes += hash
            hash
        }
        withAttachmentMeta(attachmentMetaBuilder)
    }

    suspend fun withFileBytes(size: Int): ByteArray = withFileBytes(attachmentFileStorage, fileUri, size)

    suspend fun withFileSize(size: Int) {
        whenever(attachmentFileStorage.sizeOf(fileUri)).thenReturn(size.bytes)
    }

    fun withVideoChunks(fromChunk: Int, slices: List<ByteArray>) {
        whenever(attachmentFileStorage.readChunks(fileUri, fromChunk)).thenReturn(flowOf(*slices.toTypedArray()))
    }

    suspend fun withUploadStillActive() = withUploadStillActive(fileUploadRepository)

    suspend fun verifySessionOpenedOn(nodeUrl: String) {
        verify(hopService).withSession<Any?>(eq(nodeUrl), any())
    }

    fun verifyStreamedFrom(chunk: Int) {
        verify(attachmentFileStorage).readChunks(fileUri, chunk)
    }

    fun verifyNeverStreamed() {
        verify(attachmentFileStorage, never()).readChunks(any(), anyInt())
    }

    suspend fun verifyReadWhole() {
        verify(attachmentFileStorage).readFileBytes(fileUri)
    }

    suspend fun verifyNeverReadWhole() {
        verify(attachmentFileStorage, never()).readFileBytes(any())
    }

    suspend fun verifyNeverSized() {
        verify(attachmentFileStorage, never()).sizeOf(any())
    }

    fun verifyNeverPreProcessed() {
        verify(preProcessor, never()).preProcess(any())
    }

    suspend fun verifyNoProgressRecorded() {
        verify(fileUploadRepository, never()).updateProgress(any(), anyInt(), any())
    }

    suspend fun firstRecordedProgress(): Int {
        val uploadedChunks: ArgumentCaptor<Int> = ArgumentCaptor.forClass(Int::class.java)
        verify(fileUploadRepository, atLeastOnce()).updateProgress(any(), capture(uploadedChunks), any())
        return uploadedChunks.allValues.first()
    }

    suspend fun sentAttachment(): Attachment.Hosted {
        val attachment: ArgumentCaptor<Attachment.Hosted> = ArgumentCaptor.forClass(Attachment.Hosted::class.java)
        verify(messageAttachmentUpdater).updateSentAttachment(anyChatId(), any(), capture(attachment))
        return attachment.value
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
