package io.paritytech.polkadotapp.feature_chats_impl.data.storage

import android.net.Uri
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.THREE_CHUNK_FILE_SIZE
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.chunkSlices
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.patternBytes
import io.paritytech.polkadotapp.test_shared.assertFailureOf
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import java.io.FileNotFoundException
import java.io.InputStream
import kotlin.time.Duration.Companion.seconds

class AttachmentFileStorageTest {
    private val uri: Uri = mock(Uri::class.java)
    private val fileBytes = patternBytes(THREE_CHUNK_FILE_SIZE)
    private val slices = fileBytes.chunkSlices()

    private val contentResolver: ContentResolver = mock(ContentResolver::class.java)
    private val fileProvider: FileProvider = mock(FileProvider::class.java)

    private val storage = AttachmentFileStorage(
        contentResolver = contentResolver,
        fileProvider = fileProvider,
        coroutineDispatchers = RealCoroutineDispatchers()
    )

    @Test
    fun `readChunks emits each chunk's exact slice when the last chunk is short`() = runBlocking<Unit> {
        withTrickleStream()

        val chunks = storage.readChunks(uri, fromChunk = 0).toList()

        assertChunks(slices, chunks)
    }

    @Test
    fun `readChunks skips earlier chunks when it starts from a later one`() = runBlocking<Unit> {
        withTrickleStream()

        val chunks = storage.readChunks(uri, fromChunk = 1).toList()

        assertChunks(slices.drop(1), chunks)
    }

    @Test
    fun `readChunks reads nothing ahead when the collector still holds a chunk`() = runBlocking<Unit> {
        val stream = withTrickleStream()
        var servedWhileFirstChunkHeld = -1
        var readAhead: Unit? = null

        storage.readChunks(uri, fromChunk = 0).collect { chunk ->
            if (servedWhileFirstChunkHeld < 0) {
                servedWhileFirstChunkHeld = stream.served
                readAhead = withTimeoutOrNull(READ_AHEAD_WINDOW) { stream.servedPastFirstChunk.await() }
            }
        }

        assertEquals(HopService.CHUNK_SIZE_BYTES, servedWhileFirstChunkHeld)
        assertNull(readAhead)
    }

    @Test
    fun `readChunks throws FileNotFoundException when the provider gives no stream`() = runBlocking<Unit> {
        withNoStream()

        val result = runCatching { storage.readChunks(uri, fromChunk = 0).toList() }

        assertFailureOf<FileNotFoundException>(result)
    }

    @Test
    fun `sizeOf returns the size when the provider knows it`() = runBlocking<Unit> {
        withTrickleStream()
        withProviderSize(THREE_CHUNK_FILE_SIZE.toLong())

        val size = storage.sizeOf(uri)

        assertEquals(THREE_CHUNK_FILE_SIZE.toLong(), size.inWholeBytes)
    }

    @Test
    fun `sizeOf throws when the provider does not know the size`() = runBlocking<Unit> {
        withTrickleStream()
        withUnknownProviderSize()

        val result = runCatching { storage.sizeOf(uri) }

        assertFailureOf<FileNotFoundException>(result)
    }

    @Test
    fun `readChunks closes the stream when the last chunk is emitted`() = runBlocking<Unit> {
        val stream = withTrickleStream()

        storage.readChunks(uri, fromChunk = 0).toList()

        assertTrue("expected the stream to be closed", stream.closed)
    }

    private fun withTrickleStream(): TrickleInputStream {
        val stream = TrickleInputStream(fileBytes)
        whenever(contentResolver.openInputStream(uri)).thenReturn(stream)
        return stream
    }

    private fun withNoStream() {
        whenever(contentResolver.openInputStream(uri)).thenReturn(null)
    }

    private fun withProviderSize(sizeInBytes: Long) {
        whenever(contentResolver.querySize(uri)).thenReturn(sizeInBytes.bytes)
    }

    private fun withUnknownProviderSize() {
        whenever(contentResolver.querySize(uri)).thenReturn(null)
    }

    private fun assertChunks(expected: List<ByteArray>, actual: List<ByteArray>) {
        assertEquals(expected.size, actual.size)
        expected.zip(actual).forEachIndexed { index, (expectedChunk, actualChunk) ->
            assertArrayEquals("chunk $index", expectedChunk, actualChunk)
        }
    }

    private class TrickleInputStream(private val bytes: ByteArray) : InputStream() {
        private var position = 0

        var served = 0
            private set
        var closed = false
            private set
        val servedPastFirstChunk = CompletableDeferred<Unit>()

        override fun read(): Int {
            if (position >= bytes.size) return -1
            serve(1)
            return bytes[position++].toInt() and BYTE_MASK
        }

        override fun read(b: ByteArray, off: Int, len: Int): Int {
            if (len == 0) return 0
            if (position >= bytes.size) return -1

            val count = minOf(len, MAX_BYTES_PER_CALL, bytes.size - position)
            bytes.copyInto(b, destinationOffset = off, startIndex = position, endIndex = position + count)
            position += count
            serve(count)
            return count
        }

        override fun skip(n: Long): Long {
            val count = minOf(n, MAX_BYTES_PER_CALL.toLong(), (bytes.size - position).toLong()).coerceAtLeast(0)
            position += count.toInt()
            return count
        }

        override fun available(): Int = minOf(MAX_BYTES_PER_CALL, bytes.size - position)

        override fun close() {
            closed = true
        }

        private fun serve(count: Int) {
            served += count
            if (served > HopService.CHUNK_SIZE_BYTES) servedPastFirstChunk.complete(Unit)
        }
    }

    private companion object {
        const val MAX_BYTES_PER_CALL = 65_536
        const val BYTE_MASK = 0xFF
        val READ_AHEAD_WINDOW = 1.seconds
    }
}
