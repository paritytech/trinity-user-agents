package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom

import android.net.Uri
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentDraft
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.FileTooLargeException
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream

class ProductAttachmentEncoderTest {
    private val uri = mockk<Uri>()
    private val contentResolver = mockk<ContentResolver>()
    private val metaBuilder = mockk<AttachmentMetaBuilder>()
    private var shrunk: ShrunkImage? = null
    private val encoder = ProductAttachmentEncoder(
        contentResolver = contentResolver,
        metaBuilder = metaBuilder,
        imageShrinker = { _, _ -> shrunk },
        dispatchers = TestCoroutineDispatchers(Dispatchers.Unconfined),
    )

    @Test
    fun `an image under the limit goes as it is, with its size, dimensions and BlurHash`() = runBlocking {
        val bytes = ByteArray(1_000) { 1 }
        given(bytes, Attachment.Meta.Image(width = 40, height = 30, blurHash = "LKO2", mimeType = "image/png", size = bytes.size.bytes))

        val image = encoder.encode(uri, "image/png", bytes.size.bytes).getOrThrow() as ChatMediaItemModel.Image

        assertArrayEquals(bytes, (image.resource as ChatResourceModel.Inline).bytes.value)
        assertEquals("image/png", image.meta.mimeType)
        assertEquals(1_000L, image.meta.sizeBytes)
        assertEquals(40, image.meta.width)
        assertEquals(30, image.meta.height)
        assertEquals("LKO2", image.meta.placeholder)
    }

    @Test
    fun `a larger image is encoded again until it fits`() = runBlocking {
        val bytes = ByteArray(ProductAttachmentEncoder.LIMIT_BYTES + 1)
        given(bytes, Attachment.Meta.Image(width = 4_000, height = 3_000, blurHash = null, mimeType = "image/png", size = bytes.size.bytes))
        shrunk = ShrunkImage(ByteArray(500), "image/jpeg", width = 1_440, height = 1_080)

        val image = encoder.encode(uri, "image/png", bytes.size.bytes).getOrThrow() as ChatMediaItemModel.Image

        assertEquals("image/jpeg", (image.resource as ChatResourceModel.Inline).mimeType)
        assertEquals(500L, image.meta.sizeBytes)
        assertEquals(1_440, image.meta.width)
    }

    @Test
    fun `an image that does not fit is refused`() = runBlocking {
        val bytes = ByteArray(ProductAttachmentEncoder.LIMIT_BYTES + 1)
        given(bytes, Attachment.Meta.Image(width = 1, height = 1, blurHash = null, mimeType = "image/png", size = bytes.size.bytes))

        val failure = encoder.encode(uri, "image/png", bytes.size.bytes).exceptionOrNull()

        assertTrue(failure is FileTooLargeException)
    }

    @Test
    fun `a video over the limit is refused without reading it`() = runBlocking {
        val size = (ProductAttachmentEncoder.LIMIT_BYTES + 1).bytes
        coEvery { metaBuilder.build(uri, "video/mp4", size) } returns
            Attachment.Meta.Video(duration = 1_000, blurHash = null, mimeType = "video/mp4", size = size)

        val failure = encoder.encode(uri, "video/mp4", size).exceptionOrNull()

        assertTrue(failure is FileTooLargeException)
        verify(exactly = 0) { contentResolver.openInputStream(any()) }
    }

    @Test
    fun `a file goes inline with its name`() = runBlocking {
        val bytes = "report".toByteArray()
        given(bytes, Attachment.Meta.General(fileName = "report.pdf", mimeType = "application/pdf", size = bytes.size.bytes))

        val file = encoder.encode(uri, "application/pdf", bytes.size.bytes).getOrThrow() as ChatMediaItemModel.File

        assertEquals("report.pdf", file.fileName)
        assertEquals("application/pdf", file.meta.mimeType)
        assertArrayEquals(bytes, (file.resource as ChatResourceModel.Inline).bytes.value)
    }

    @Test
    fun `only an attachment above 1 MiB goes to staging, and an image above 8 MiB goes inline again`() {
        val above = (ProductAttachmentEncoder.LIMIT_BYTES + 1).bytes
        val huge = (ProductAttachmentEncoder.STAGED_LIMIT_BYTES + 1).bytes

        assertFalse(encoder.goesToStaging("image/png", ProductAttachmentEncoder.LIMIT))
        assertTrue(encoder.goesToStaging("image/png", above))
        assertTrue(encoder.goesToStaging("application/pdf", ProductAttachmentEncoder.STAGED_LIMIT))
        assertTrue(encoder.goesToStaging("application/pdf", huge))
        assertFalse(encoder.goesToStaging("image/png", huge))
    }

    @Test
    fun `a staged image keeps its original bytes and its meta`() = runBlocking {
        val bytes = ByteArray(3 * 1024 * 1024) { 3 }
        given(bytes, Attachment.Meta.Image(width = 4_000, height = 3_000, blurHash = "LKO2", mimeType = "image/jpeg", size = bytes.size.bytes))

        val draft = encoder.prepareStaged(uri, "image/jpeg", bytes.size.bytes).getOrThrow()

        assertEquals(ProductAttachmentDraft.Kind.IMAGE, draft.kind)
        assertArrayEquals(bytes, draft.bytes)
        assertEquals(bytes.size.toLong(), draft.meta.sizeBytes)
        assertEquals(4_000, draft.meta.width)
        assertEquals("LKO2", draft.meta.placeholder)
    }

    @Test
    fun `a staged file keeps its name`() = runBlocking {
        val bytes = ByteArray(6 * 1024 * 1024)
        given(bytes, Attachment.Meta.General(fileName = "", mimeType = "application/pdf", size = bytes.size.bytes))

        val draft = encoder.prepareStaged(uri, "application/pdf", bytes.size.bytes).getOrThrow()

        assertEquals(ProductAttachmentDraft.Kind.FILE, draft.kind)
        assertEquals("file", draft.fileName)
    }

    @Test
    fun `an attachment above 8 MiB is not staged, and is not read`() = runBlocking {
        val size = (ProductAttachmentEncoder.STAGED_LIMIT_BYTES + 1).bytes

        val failure = encoder.prepareStaged(uri, "application/pdf", size).exceptionOrNull()

        assertTrue(failure is FileTooLargeException)
        assertEquals(ProductAttachmentEncoder.STAGED_LIMIT, (failure as FileTooLargeException).maxSize)
        verify(exactly = 0) { contentResolver.openInputStream(any()) }
    }

    private fun given(bytes: ByteArray, meta: Attachment.Meta) {
        coEvery { metaBuilder.build(uri, meta.mimeType, bytes.size.bytes) } returns meta
        every { contentResolver.openInputStream(uri) } answers { ByteArrayInputStream(bytes) }
    }
}
