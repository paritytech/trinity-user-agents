package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.THREE_CHUNK_FILE_SIZE
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.chunkSlices
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.patternBytes
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

class HopFileUploaderTest {
    private val uploads = MockedSessionUploads()

    @Before
    fun setUp() = runBlocking<Unit> {
        uploads.withUploadAllowed()
    }

    @Test
    fun `each chunk carries exactly its slice in order when the last chunk is short`() = runBlocking<Unit> {
        val preProcessed = withChunkedImage()

        uploads.upload(fileUpload(uploads.fileUri))

        assertEquals(4, uploads.submitCount)
        uploads.assertSubmittedChunks(preProcessed.chunkSlices())
    }

    @Test
    fun `the chunked root lists every returned hash when the last chunk is short`() = runBlocking<Unit> {
        withChunkedImage()

        uploads.upload(fileUpload(uploads.fileUri))

        uploads.assertRootListsHashes(List(3) { uploads.returnedHash(it) })
    }

    @Test
    fun `an upload submits only the remaining slices when it resumes on an unchanged node`() = runBlocking<Unit> {
        val preProcessed = withChunkedImage()

        uploads.upload(resumedImage())

        assertEquals(3, uploads.submitCount)
        uploads.assertSubmittedChunks(preProcessed.chunkSlices().drop(1))
    }

    @Test
    fun `an upload lists its stored and new hashes when it resumes on an unchanged node`() = runBlocking<Unit> {
        withChunkedImage()

        uploads.upload(resumedImage())

        uploads.assertRootListsHashes(listOf(STORED_HASH_0.fromHex(), uploads.returnedHash(0), uploads.returnedHash(1)))
    }

    @Test
    fun `an upload counts its progress from the stored chunks when it resumes on an unchanged node`() = runBlocking<Unit> {
        withChunkedImage()

        uploads.upload(resumedImage())

        assertEquals(2, uploads.firstRecordedProgress())
    }

    @Test
    fun `a video is streamed without being read whole when it is above the inline size`() = runBlocking<Unit> {
        withResumableVideo()

        uploads.upload(resumedVideo())

        uploads.verifyStreamedFrom(1)
        uploads.verifyNeverReadWhole()
    }

    @Test
    fun `a video is not pre-processed when it is above the inline size`() = runBlocking<Unit> {
        withResumableVideo()

        uploads.upload(resumedVideo())

        uploads.verifyNeverPreProcessed()
    }

    @Test
    fun `a streamed video submits its remaining slices and lists every hash when it is above the inline size`() = runBlocking<Unit> {
        val remainingSlices = withResumableVideo()

        uploads.upload(resumedVideo())

        uploads.assertSubmittedChunks(remainingSlices)
        uploads.assertRootListsHashes(listOf(STORED_HASH_0.fromHex(), uploads.returnedHash(0), uploads.returnedHash(1)))
    }

    @Test
    fun `an image is read whole and pre-processed when it is above the inline size`() = runBlocking<Unit> {
        val raw = withLargeImage()

        uploads.upload(fileUpload(uploads.fileUri))

        uploads.verifyReadWhole()
        assertArrayEquals(raw + MARKER_BYTE, uploads.chunkPlaintexts().reduce { acc, chunk -> acc + chunk })
    }

    @Test
    fun `an image is neither streamed nor sized when it is above the inline size`() = runBlocking<Unit> {
        withLargeImage()

        uploads.upload(fileUpload(uploads.fileUri))

        uploads.verifyNeverStreamed()
        uploads.verifyNeverSized()
    }

    @Test
    fun `a streamed upload submits only the chunked root when every chunk was already uploaded`() = runBlocking<Unit> {
        withFullyUploadedVideo()

        uploads.upload(fullyUploadedVideo())

        assertEquals(1, uploads.submitCount)
        uploads.assertRootListsHashes(ALL_STORED_HASHES.map { it.fromHex() })
    }

    @Test
    fun `a streamed upload reads nothing when every chunk was already uploaded`() = runBlocking<Unit> {
        withFullyUploadedVideo()

        uploads.upload(fullyUploadedVideo())

        uploads.verifyNeverStreamed()
        uploads.verifyNeverReadWhole()
    }

    @Test
    fun `a streamed upload records no progress when every chunk was already uploaded`() = runBlocking<Unit> {
        withFullyUploadedVideo()

        uploads.upload(fullyUploadedVideo())

        uploads.verifyNoProgressRecorded()
    }

    @Test
    fun `a video is read whole when it is exactly the inline size`() = runBlocking<Unit> {
        withVideoOfExactlyTheInlineSize()

        uploads.upload(fileUpload(uploads.fileUri, mimeType = VIDEO_MP4))

        uploads.verifyReadWhole()
        uploads.verifyNeverStreamed()
    }

    @Test
    fun `a video is sent inline when it is exactly the inline size`() = runBlocking<Unit> {
        withVideoOfExactlyTheInlineSize()

        uploads.upload(fileUpload(uploads.fileUri, mimeType = VIDEO_MP4))

        assertEquals(1, uploads.submitCount)
    }

    @Test
    fun `a video is streamed when it is one byte over the inline size`() = runBlocking<Unit> {
        uploads.withSuccessfulSession()
        uploads.withUploadStillActive()
        uploads.withFileSize(HopService.INLINE_MAX_BYTES + 1)
        uploads.withVideoChunks(fromChunk = 0, patternBytes(HopService.INLINE_MAX_BYTES + 1).chunkSlices())

        uploads.upload(fileUpload(uploads.fileUri, mimeType = VIDEO_MP4))

        uploads.verifyNeverReadWhole()
        assertEquals(2, uploads.submitCount)
    }

    private suspend fun withChunkedImage(): ByteArray {
        uploads.withSuccessfulSession()
        uploads.withUploadStillActive()
        return uploads.withFileBytes(THREE_CHUNK_FILE_SIZE - 1) + MARKER_BYTE
    }

    private fun resumedImage() = fileUpload(uploads.fileUri, uploadedChunkHashes = listOf(STORED_HASH_0))

    private suspend fun withResumableVideo(): List<ByteArray> {
        uploads.withSuccessfulSession()
        uploads.withUploadStillActive()
        uploads.withFileSize(THREE_CHUNK_FILE_SIZE)
        val remainingSlices = patternBytes(THREE_CHUNK_FILE_SIZE).chunkSlices().drop(1)
        uploads.withVideoChunks(fromChunk = 1, remainingSlices)
        return remainingSlices
    }

    private fun resumedVideo() = fileUpload(uploads.fileUri, mimeType = VIDEO_MP4, uploadedChunkHashes = listOf(STORED_HASH_0))

    private suspend fun withLargeImage(): ByteArray {
        uploads.withSuccessfulSession()
        uploads.withUploadStillActive()
        uploads.withFileSize(LARGE_IMAGE_SIZE)
        return uploads.withFileBytes(LARGE_IMAGE_SIZE)
    }

    private suspend fun withFullyUploadedVideo() {
        uploads.withSuccessfulSession()
        uploads.withUploadStillActive()
        uploads.withFileSize(THREE_CHUNK_FILE_SIZE)
    }

    private fun fullyUploadedVideo() = fileUpload(uploads.fileUri, mimeType = VIDEO_MP4, uploadedChunkHashes = ALL_STORED_HASHES)

    private suspend fun withVideoOfExactlyTheInlineSize() {
        uploads.withSuccessfulSession()
        uploads.withFileSize(HopService.INLINE_MAX_BYTES)
        uploads.withFileBytes(HopService.INLINE_MAX_BYTES)
    }

    private companion object {
        val ALL_STORED_HASHES = listOf(STORED_HASH_0, STORED_HASH_1, STORED_HASH_2)
    }
}
