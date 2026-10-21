package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatHopFile
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatHopFiles
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatHopFileCache
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatResourceDiskCache
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatSealedFileStore
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.IOException
import java.security.MessageDigest
import java.util.Base64

class RealChatMediaLoaderTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val ipfsContentLookup = mockk<IpfsContentLookup>()
    private val hopFiles = mockk<ChatHopFiles>()
    private val loader by lazy {
        RealChatMediaLoader(
            ipfsContentLookup = ipfsContentLookup,
            cache = ChatResourceDiskCache(folder.root, maxBytes = 1_024 * 1_024),
            dispatchers = TestCoroutineDispatchers(UnconfinedTestDispatcher()),
            sealedFiles = ChatSealedFileStore(ChatResourceDiskCache(folder.newFolder("sealed"), maxBytes = 1_024 * 1_024)),
            hopFiles = hopFiles,
            hopCache = ChatHopFileCache(ChatSealedFileStore(ChatResourceDiskCache(folder.newFolder("hop"), maxBytes = 1_024 * 1_024))),
        )
    }

    private val preimage = ChatResourceModel.Preimage(ByteArray(32) { 7 }.toDataByteArray())

    @Test
    fun `inline bytes go to one file with the extension of their type`() = runBlocking<Unit> {
        val inline = ChatResourceModel.Inline("video/mp4", byteArrayOf(1, 2).toDataByteArray())

        val first = loader.load(inline, "mp4").getOrThrow()
        val second = loader.load(inline, "mp4").getOrThrow()

        assertEquals(first, second)
        assertEquals("mp4", first.extension)
        assertArrayEquals(byteArrayOf(1, 2), first.readBytes())
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
    fun `a sealed file opens from its parts and stays after the chain drops them`() = runBlocking<Unit> {
        val ciphertext = SEALED_CIPHERTEXT.fromHex()
        val sealed = ChatResourceModel.Sealed(
            parts = listOf(ciphertext.blake2b256().toDataByteArray()),
            key = ByteArray(32) { it.toByte() }.toDataByteArray(),
            nonce = ByteArray(24) { (100 + it).toByte() }.toDataByteArray(),
            size = ciphertext.size.toLong(),
            digest = MessageDigest.getInstance("SHA-512").digest(ciphertext).toDataByteArray(),
        )
        coEvery { ipfsContentLookup.lookupRawHash(any()) } returnsMany listOf(Result.success(ciphertext), Result.failure(IOException("gone")))

        val first = loader.load(sealed, "jpg").getOrThrow()
        val second = loader.load(sealed, "jpg").getOrThrow()

        assertEquals("a sealed attachment of the chat", first.readText())
        assertEquals(first, second)
        coVerify(exactly = 1) { ipfsContentLookup.lookupRawHash(any()) }
    }

    @Test
    fun `a preimage is looked up once and then read from the cache`() = runBlocking<Unit> {
        coEvery { ipfsContentLookup.lookupRawHash(any()) } returns Result.success(byteArrayOf(9))

        loader.load(preimage, "png").getOrThrow()
        val cached = loader.load(preimage, "png").getOrThrow()

        assertArrayEquals(byteArrayOf(9), cached.readBytes())
        coVerify(exactly = 1) { ipfsContentLookup.lookupRawHash(any()) }
    }

    @Test
    fun `a missing preimage fails, and a retry asks the lookup again`() = runBlocking<Unit> {
        coEvery { ipfsContentLookup.lookupRawHash(any()) } returnsMany listOf(
            Result.failure(IOException("IPFS fetch failed: HTTP 404")),
            Result.success(byteArrayOf(4)),
        )

        assertTrue(loader.load(preimage, "png").isFailure)
        assertArrayEquals(byteArrayOf(4), loader.load(preimage, "png").getOrThrow().readBytes())
    }

    @Test
    fun `a data address decodes, and an https address is not fetched`() = runBlocking<Unit> {
        val data = "data:image/png;base64," + Base64.getEncoder().encodeToString(byteArrayOf(5, 6))

        assertArrayEquals(byteArrayOf(5, 6), loader.load(ChatResourceModel.Url(data), "png").getOrThrow().readBytes())
        assertTrue(loader.load(ChatResourceModel.Url("https://example.com/a.png"), "png").isFailure)
    }

    private companion object {
        const val SEALED_CIPHERTEXT = "1d538ad59e42ece6a751e94bc70cd5a43ed526518b4aeac4e43044c4433f0cfa4dc9a5e93f656e666e4a97c2950479"
    }
}
