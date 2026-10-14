package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.InputStream

/**
 * A dev server is not a product archive, so nothing here is reachable from a published manifest. It
 * is still a remote the host does not control, and the host reads it before the user has approved
 * anything.
 */
class RemoteFaceBoundTest {
    @Test
    fun `a face within the bound is read whole`() {
        val face = "x".repeat(1024)

        assertEquals(face, face.asStream().readFaceWithinBound())
    }

    @Test
    fun `a face exactly on the bound is still read`() {
        val face = "x".repeat(MAX_FACE_BYTES.toInt())

        assertEquals(face, face.asStream().readFaceWithinBound())
    }

    /**
     * The read goes one byte past the bound rather than trusting a declared length, so a server that
     * declares nothing — or lies — is refused instead of read until the device runs out of memory.
     */
    @Test
    fun `a face one byte over the bound is refused`() {
        val face = "x".repeat(MAX_FACE_BYTES.toInt() + 1)

        assertThrows(IllegalArgumentException::class.java) { face.asStream().readFaceWithinBound() }
    }

    @Test
    fun `an endless stream is refused rather than drained`() {
        val endless = object : InputStream() {
            override fun read(): Int = 'x'.code
        }

        assertThrows(IllegalArgumentException::class.java) { endless.readFaceWithinBound() }
    }

    private fun String.asStream() = ByteArrayInputStream(toByteArray())
}
