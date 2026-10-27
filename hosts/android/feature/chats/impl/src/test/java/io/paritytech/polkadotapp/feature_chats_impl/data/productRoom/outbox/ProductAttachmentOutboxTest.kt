package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
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
    fun `a text message and what waits for it stay after the process starts again`() = runBlocking {
        val first = outbox()
        val added = first.addText(room, "hello", replyTo = "m0")
        first.update(added.id) { it.copy(editedText = "hello there", editedAt = 77, reaction = "👍") }

        val read = outbox().get(added.id)!!

        assertEquals("hello", read.text)
        assertEquals("m0", read.replyTo)
        assertEquals("hello there", read.editedText)
        assertEquals(77L, read.editedAt)
        assertEquals("👍", read.reaction)
    }

    @Test
    fun `each failure is kept`() = runBlocking {
        val outbox = outbox()
        val refused = outbox.addText(room, "refused", replyTo = null)
        val failed = outbox.addText(room, "failed", replyTo = null)
        outbox.update(refused.id) { it.copy(failure = ProductAttachmentFailure.Refused("no")) }
        outbox.update(failed.id) { it.copy(failure = ProductAttachmentFailure.Failed("lost"), refusals = 5) }

        val read = outbox()

        assertEquals(ProductAttachmentFailure.Refused("no"), read.get(refused.id)!!.failure)
        assertEquals(ProductAttachmentFailure.Failed("lost"), read.get(failed.id)!!.failure)
        assertEquals(5, read.get(failed.id)!!.refusals)
    }

    @Test
    fun `a room lists its own entries, the oldest first`() = runBlocking {
        val outbox = outbox()
        val first = outbox.addText(room, "first", replyTo = null)
        outbox.addText(otherRoom, "elsewhere", replyTo = null)
        val second = outbox.addText(room, "second", replyTo = null)

        assertEquals(listOf(first.id, second.id), outbox.observe(room).first().map { it.id })
    }

    @Test
    fun `an update of a removed entry does nothing`() = runBlocking {
        val outbox = outbox()
        val added = outbox.addText(room, "hello", replyTo = null)
        assertTrue(outbox.remove(added.id))

        assertNull(outbox.update(added.id) { it.copy(reaction = "👍") })
        assertEquals(emptyList<ProductOutboxEntry>(), outbox().all())
        assertFalse(outbox.remove(added.id))
    }

    @Test
    fun `remove only an unchanged entry`() = runBlocking {
        val outbox = outbox()
        val added = outbox.addText(room, "hello", replyTo = null)
        outbox.update(added.id) { it.copy(reaction = "👍") }

        assertFalse(outbox.removeIfUnchanged(added))
        assertTrue(outbox.removeIfUnchanged(outbox.get(added.id)!!))
        assertNull(outbox.get(added.id))
    }

    @Test
    fun `an entry that is not a text message is removed`() = runBlocking {
        val added = outbox().addText(room, "hello", replyTo = null)
        val entry = folder.root.resolve(added.id).resolve("entry.properties")
        entry.writeText(entry.readText().replace("upload=NONE", "upload=HOP"))

        assertEquals(emptyList<ProductOutboxEntry>(), outbox().all())
    }

    @Test
    fun `an entry that cannot be read is removed`() = runBlocking {
        folder.newFolder("broken").resolve("entry.properties").writeText("id=broken\n")

        assertEquals(emptyList<ProductOutboxEntry>(), outbox().all())
        assertFalse(folder.root.resolve("broken").exists())
    }
}
