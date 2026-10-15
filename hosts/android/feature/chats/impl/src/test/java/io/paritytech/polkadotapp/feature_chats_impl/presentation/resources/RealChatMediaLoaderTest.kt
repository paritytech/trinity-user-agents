package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatResourceDiskCache
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
import java.util.Base64

class RealChatMediaLoaderTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val ipfsContentLookup = mockk<IpfsContentLookup>()
    private val loader by lazy {
        RealChatMediaLoader(
            ipfsContentLookup = ipfsContentLookup,
            cache = ChatResourceDiskCache(folder.root, maxBytes = 1_024 * 1_024),
            dispatchers = TestCoroutineDispatchers(UnconfinedTestDispatcher()),
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
}
