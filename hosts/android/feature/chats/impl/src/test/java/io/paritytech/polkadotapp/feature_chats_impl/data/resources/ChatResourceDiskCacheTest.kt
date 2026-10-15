package io.paritytech.polkadotapp.feature_chats_impl.data.resources

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ChatResourceDiskCacheTest {
    @get:Rule
    val folder = TemporaryFolder()

    private var clock = 1_000_000L
    private fun cache(maxBytes: Long) = ChatResourceDiskCache(folder.root.resolve("chat_resources"), maxBytes, now = { clock++ * 1_000 })

    @Test
    fun `a stored resource reads back from its file`() {
        val cache = cache(maxBytes = 1_024)

        val stored = cache.store("preimage:01", "png", byteArrayOf(1, 2, 3)).getOrThrow()

        assertEquals("png", stored.extension)
        assertArrayEquals(byteArrayOf(1, 2, 3), cache.file("preimage:01", "png")!!.readBytes())
        assertNull(cache.file("preimage:02", "png"))
    }

    @Test
    fun `the cache removes the resources that were read longest ago`() {
        val cache = cache(maxBytes = 300)
        cache.store("a", null, ByteArray(100)).getOrThrow()
        cache.store("b", null, ByteArray(100)).getOrThrow()
        cache.store("c", null, ByteArray(100)).getOrThrow()

        assertNotNull(cache.file("a", null))
        cache.store("d", null, ByteArray(100)).getOrThrow()

        assertNotNull(cache.file("a", null))
        assertNull(cache.file("b", null))
        assertNotNull(cache.file("c", null))
        assertNotNull(cache.file("d", null))
        assertTrue(cache.sizeBytes() <= 300)
    }

    @Test
    fun `a resource that is larger than the bound is not kept`() {
        val cache = cache(maxBytes = 10)

        assertTrue(cache.store("big", null, ByteArray(11)).isFailure)
        assertEquals(0L, cache.sizeBytes())
    }
}
