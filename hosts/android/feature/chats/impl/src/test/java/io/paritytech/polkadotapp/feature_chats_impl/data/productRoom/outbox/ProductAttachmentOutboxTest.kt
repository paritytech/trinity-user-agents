package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ProductAttachmentOutboxTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val room = ChatId.forExtensionRoom("ProductBot_sample.dot", "host")
    private val otherRoom = ChatId.forExtensionRoom("ProductBot_sample.dot", "source")
    private var clock = 1_000L

    private fun outbox() = ProductAttachmentOutbox(folder.root, Dispatchers.Unconfined) { clock++ }

    @Test
    fun `an entry and its bytes stay after the process starts again`() = runBlocking {
        val bytes = ByteArray(3 * 1024 * 1024) { (it % 251).toByte() }
        val added = outbox().add(room, draft(bytes, ProductAttachmentDraft.Kind.IMAGE), text = "a caption", replyTo = "reply-1", upload = ProductAttachmentUpload.PREIMAGE)

        val reopened = outbox()
        val read = reopened.get(added.id)

        assertEquals(added, read)
        assertTrue(ProductAttachmentOutbox.isOutboxId(added.id))
        assertArrayEquals(bytes, reopened.bytes(added.id))
        assertEquals(listOf(added), reopened.observe(room).first())
        assertEquals(emptyList<ProductOutboxEntry>(), reopened.observe(otherRoom).first())
    }

    @Test
    fun `the key and each failure are kept`() = runBlocking {
        val first = outbox()
        val added = first.add(room, draft(ByteArray(10), ProductAttachmentDraft.Kind.FILE), text = null, replyTo = null, upload = ProductAttachmentUpload.PREIMAGE)
        val key = ByteArray(32) { 7 }.toDataByteArray()
        val failures = listOf(
            ProductAttachmentFailure.TooLarge(2_097_152),
            ProductAttachmentFailure.NotDeclared,
            ProductAttachmentFailure.PermissionDenied,
            ProductAttachmentFailure.AllowanceUnavailable("no slot"),
            ProductAttachmentFailure.Failed("timeout"),
            ProductAttachmentFailure.Refused(null),
            ProductAttachmentFailure.Refused("not today"),
        )

        for (failure in failures) {
            first.update(added.id) { it.copy(key = key, failure = failure) }

            val read = outbox().get(added.id)!!
            assertEquals(failure, read.failure)
            assertArrayEquals(key.value, read.key!!.value)
        }

        first.update(added.id) { it.copy(failure = null) }
        assertFalse(outbox().get(added.id)!!.isFailed)
    }

    @Test
    fun `remove deletes the bytes`() = runBlocking {
        val outbox = outbox()
        val added = outbox.add(room, draft(ByteArray(10), ProductAttachmentDraft.Kind.VIDEO), text = null, replyTo = null, upload = ProductAttachmentUpload.PREIMAGE)
        assertTrue(outbox.bytesFile(added.id).isFile)

        outbox.remove(added.id)

        assertNull(outbox.get(added.id))
        assertNull(outbox.bytes(added.id))
        assertFalse(outbox.bytesFile(added.id).exists())
        assertEquals(emptyList<ProductOutboxEntry>(), outbox().all())
    }

    @Test
    fun `an update of a removed entry does nothing`() = runBlocking {
        val outbox = outbox()
        val added = outbox.add(room, draft(ByteArray(10), ProductAttachmentDraft.Kind.FILE), text = null, replyTo = null, upload = ProductAttachmentUpload.PREIMAGE)
        outbox.remove(added.id)

        assertNull(outbox.update(added.id) { it.copy(failure = ProductAttachmentFailure.NotDeclared) })
        assertEquals(emptyList<ProductOutboxEntry>(), outbox().all())
    }

    @Test
    fun `an entry that cannot be read is removed`() = runBlocking {
        folder.newFolder("broken").resolve("entry.properties").writeText("id=outbox:broken\n")

        assertEquals(emptyList<ProductOutboxEntry>(), outbox().all())
        assertFalse(folder.root.resolve("broken").exists())
    }

    private fun draft(bytes: ByteArray, kind: ProductAttachmentDraft.Kind) = ProductAttachmentDraft(
        kind = kind,
        bytes = bytes,
        meta = ChatMediaMetaModel(
            mimeType = "image/png",
            sizeBytes = bytes.size.toLong(),
            width = 40,
            height = 30,
            placeholder = "LKO2",
            alt = null,
        ),
        fileName = if (kind == ProductAttachmentDraft.Kind.FILE) "report.pdf" else null,
    )
}
