package io.paritytech.polkadotapp.common.utils

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class ByteArraysTest {
    @Test
    fun `the last chunk is shorter when the size is not a multiple of the chunk size`() {
        val chunks = ByteArray(5) { it.toByte() }.chunkedSequence(chunkSize = 2).toList()

        assertChunks(listOf(byteArrayOf(0, 1), byteArrayOf(2, 3), byteArrayOf(4)), chunks)
    }

    @Test
    fun `an empty array has no chunks when it is chunked`() {
        val chunks = ByteArray(0).chunkedSequence(chunkSize = 2).toList()

        assertEquals(0, chunks.size)
    }

    private fun assertChunks(expected: List<ByteArray>, actual: List<ByteArray>) {
        assertEquals(expected.size, actual.size)
        expected.zip(actual).forEach { (expectedChunk, actualChunk) -> assertArrayEquals(expectedChunk, actualChunk) }
    }
}
