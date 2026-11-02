package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CompressImagesTest {
    private val compressImages = CompressImages()

    @Test
    fun `applies when the file is a JPEG`() {
        assertTrue(compressImages.appliesTo(IMAGE_JPEG))
    }

    @Test
    fun `applies when the file is a PNG`() {
        assertTrue(compressImages.appliesTo("image/png"))
    }

    @Test
    fun `does not apply when the file is a GIF`() {
        assertFalse(compressImages.appliesTo("image/gif"))
    }

    @Test
    fun `does not apply when the file is a video`() {
        assertFalse(compressImages.appliesTo(VIDEO_MP4))
    }

    @Test
    fun `does not apply when the file is untyped`() {
        assertFalse(compressImages.appliesTo("application/octet-stream"))
    }
}
