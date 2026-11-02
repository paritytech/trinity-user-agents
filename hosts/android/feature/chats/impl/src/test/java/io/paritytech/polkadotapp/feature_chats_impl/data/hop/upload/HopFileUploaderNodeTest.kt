package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import io.paritytech.polkadotapp.feature_chats_impl.data.hop.NODE_B
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.THREE_CHUNK_FILE_SIZE
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.chunkSlices
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.patternBytes
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

class HopFileUploaderNodeTest {
    private val uploads = MockedSessionUploads()

    @Before
    fun setUp() = runBlocking<Unit> {
        uploads.withUploadAllowed()
    }

    @Test
    fun `an upload opens its session on its stored node when it starts`() = runBlocking<Unit> {
        withThreeChunkFile()

        uploads.upload(uploadOnNodeBWithoutStoredChunks())

        uploads.verifySessionOpenedOn(NODE_B)
    }

    @Test
    fun `the message names the stored node when the upload finishes`() = runBlocking<Unit> {
        withThreeChunkFile()

        uploads.upload(uploadOnNodeBWithoutStoredChunks())

        assertEquals(NODE_B, uploads.sentAttachment().nodeUrl)
    }

    @Test
    fun `an upload submits every chunk when it has no stored chunks`() = runBlocking<Unit> {
        withThreeChunkFile()

        uploads.upload(uploadOnNodeBWithoutStoredChunks())

        uploads.verifyStreamedFrom(0)
        assertEquals(4, uploads.submitCount)
    }

    @Test
    fun `an upload counts its progress from the first chunk when it has no stored chunks`() = runBlocking<Unit> {
        withThreeChunkFile()

        uploads.upload(uploadOnNodeBWithoutStoredChunks())

        assertEquals(1, uploads.firstRecordedProgress())
    }

    @Test
    fun `an upload lists only the new hashes when it has no stored chunks`() = runBlocking<Unit> {
        withThreeChunkFile()
        val chunkCount = patternBytes(THREE_CHUNK_FILE_SIZE).chunkSlices().size

        uploads.upload(uploadOnNodeBWithoutStoredChunks())

        uploads.assertRootListsHashes(List(chunkCount) { uploads.returnedHash(it) })
    }

    private suspend fun withThreeChunkFile() {
        uploads.withSuccessfulSession()
        uploads.withUploadStillActive()
        uploads.withFileSize(THREE_CHUNK_FILE_SIZE)
        uploads.withVideoChunks(fromChunk = 0, patternBytes(THREE_CHUNK_FILE_SIZE).chunkSlices())
        uploads.withVideoChunks(fromChunk = 1, patternBytes(THREE_CHUNK_FILE_SIZE).chunkSlices().drop(1))
    }

    private fun uploadOnNodeBWithoutStoredChunks() = fileUpload(
        uri = uploads.fileUri,
        mimeType = VIDEO_MP4,
        nodeUrl = NODE_B
    )
}
