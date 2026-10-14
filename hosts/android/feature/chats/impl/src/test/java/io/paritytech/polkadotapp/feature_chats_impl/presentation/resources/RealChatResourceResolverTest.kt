package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Base64

class RealChatResourceResolverTest {
    private val ipfsContentLookup = mockk<IpfsContentLookup>()
    private val resolver = RealChatResourceResolver(ipfsContentLookup)

    private val bytes = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)

    @Test
    fun `inline bytes are drawn as they are`() = runBlocking<Unit> {
        val result = resolver.resolve(ChatResourceModel.Inline("image/png", bytes.toDataByteArray())).getOrThrow()

        assertArrayEquals(bytes, (result as ResolvedChatImage.Inline).bytes)
    }

    @Test
    fun `a preimage resolves to the raw block address of its key`() = runBlocking<Unit> {
        withGatewayAddress("https://ipfs.example/ipfs/bafkkey")

        val result = resolver.resolve(ChatResourceModel.Preimage(ByteArray(32) { 3 }.toDataByteArray())).getOrThrow()

        assertEquals(ResolvedChatImage.Remote("https://ipfs.example/ipfs/bafkkey"), result)
    }

    @Test
    fun `a preimage that the gateway cannot address fails`() = runBlocking<Unit> {
        coEvery { ipfsContentLookup.getIpfsLinkFor(any(), any()) } returns Result.failure(IllegalStateException("no gateway"))

        assertTrue(resolver.resolve(ChatResourceModel.Preimage(ByteArray(32).toDataByteArray())).isFailure)
    }

    @Test
    fun `an address of a first version body resolves as an icon`() = runBlocking<Unit> {
        val result = resolver.resolve(ChatResourceModel.Url("https://example.com/photo.jpg")).getOrThrow()

        assertEquals(ResolvedChatImage.Remote("https://example.com/photo.jpg"), result)
    }

    @Test
    fun `a base64 data icon is decoded`() {
        val icon = "data:image/png;base64," + Base64.getEncoder().encodeToString(bytes)

        val result = resolver.resolveIcon(icon).getOrThrow()

        assertArrayEquals(bytes, (result as ResolvedChatImage.Inline).bytes)
    }

    @Test
    fun `a data icon that is not base64 fails`() {
        assertTrue(resolver.resolveIcon("data:image/svg+xml,<svg/>").isFailure)
    }

    @Test
    fun `an https icon goes to the image loader`() {
        assertEquals(
            ResolvedChatImage.Remote("https://example.com/icon.png"),
            resolver.resolveIcon("https://example.com/icon.png").getOrThrow(),
        )
    }

    private fun withGatewayAddress(address: String) {
        coEvery { ipfsContentLookup.getIpfsLinkFor(any(), any()) } returns Result.success(address)
    }
}
