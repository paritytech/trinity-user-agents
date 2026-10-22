package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatHopFile
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatHopFiles
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatHopFileCache
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatHopFileStore
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatResourceDiskCache
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.IOException
import java.util.Base64

class RealChatMediaLoaderTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val hopFiles = mockk<ChatHopFiles>()
    private val loader by lazy {
        RealChatMediaLoader(
            cache = ChatResourceDiskCache(folder.root, maxBytes = 1_024 * 1_024),
            dispatchers = TestCoroutineDispatchers(UnconfinedTestDispatcher()),
            hopFiles = hopFiles,
            hopCache = ChatHopFileCache(ChatHopFileStore(ChatResourceDiskCache(folder.newFolder("hop"), maxBytes = 1_024 * 1_024))),
        )
    }

    @Test
    fun `a HOP file is opened once, kept, and only then acked so the node drops it`() = runBlocking<Unit> {
        val hop = ChatResourceModel.Hop(
            identifier = ByteArray(32) { 1 }.toDataByteArray(),
            ticket = ByteArray(32) { 2 }.toDataByteArray(),
            node = "wss://hop.example",
            size = 3,
        )
        val entries = listOf(ByteArray(32) { 1 })
        coEvery { hopFiles.open(hop) } returns Result.success(ChatHopFile(byteArrayOf(4, 5, 6), entries))
        coEvery { hopFiles.ack(hop, entries) } returns Result.success(Unit)

        val first = loader.load(hop, "jpg").getOrThrow()
        val second = loader.load(hop, "jpg").getOrThrow()

        assertArrayEquals(byteArrayOf(4, 5, 6), first.readBytes())
        assertEquals(first, second)
        coVerify(exactly = 1) { hopFiles.open(hop) }
        coVerify(exactly = 1) { hopFiles.ack(hop, entries) }
    }

    @Test
    fun `a HOP file the core cannot open is not acked and is tried again later`() = runBlocking<Unit> {
        val hop = ChatResourceModel.Hop(ByteArray(32).toDataByteArray(), ByteArray(32).toDataByteArray(), "wss://hop.example", 1)
        coEvery { hopFiles.open(hop) } returns Result.failure(IOException("node unreachable"))

        assertTrue(loader.load(hop, "jpg").isFailure)
        assertTrue(loader.load(hop, "jpg").isFailure)
        coVerify(exactly = 2) { hopFiles.open(hop) }
        coVerify(exactly = 0) { hopFiles.ack(any(), any()) }
    }

    @Test
    fun `a data address decodes, and an https address is not fetched`() = runBlocking<Unit> {
        val data = "data:image/png;base64," + Base64.getEncoder().encodeToString(byteArrayOf(5, 6))

        assertArrayEquals(byteArrayOf(5, 6), loader.load(ChatResourceModel.Url(data), "png").getOrThrow().readBytes())
        assertTrue(loader.load(ChatResourceModel.Url("https://example.com/a.png"), "png").isFailure)
    }
}
