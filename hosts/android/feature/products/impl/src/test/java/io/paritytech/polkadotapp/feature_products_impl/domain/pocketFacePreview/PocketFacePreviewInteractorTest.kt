package io.paritytech.polkadotapp.feature_products_impl.domain.pocketFacePreview

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RemoteFaceSource
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private const val FACE_URL = "http://127.0.0.1:5173/pocket/loyalty.json"
private const val GATEWAY_LINK = "https://gateway.example/ipfs/bafy"

class PocketFacePreviewInteractorTest {
    private val gateway = mockk<IpfsContentLookup> {
        coEvery { getIpfsLinkFor("bafy") } returns Result.success(GATEWAY_LINK)
    }

    private val interactor = PocketFacePreviewInteractor(mockk<RemoteFaceSource>(), gateway)

    private suspend fun archiveImage(path: String, faceUrl: String = FACE_URL) =
        interactor.resolveImage(faceUrl, JsImageSource.Archive(path))

    /**
     * A published manifest names images relative to the archive root, and a developer serves the folder that
     * becomes the archive. Resolving against the server root keeps the face identical in both places.
     */
    @Test
    fun `an archive image is read from the face's server at its archive path`() = runTest {
        assertEquals(
            "http://127.0.0.1:5173/images/stamp.png",
            archiveImage("images/stamp.png").getOrThrow(),
        )
        assertEquals(
            "http://127.0.0.1:5173/images/stamp.png",
            archiveImage("images/stamp.png", "$FACE_URL?cachebust=1#top").getOrThrow(),
        )
    }

    /**
     * A card reads an archive path as a file inside the archive, not as a URL. The preview must find the same
     * file, or it draws an image where the card has none.
     */
    @Test
    fun `an archive path is read as a file in the archive, as a card reads it`() = runTest {
        assertEquals("http://127.0.0.1:5173/images/stamp.png", archiveImage("images/badges/../stamp.png").getOrThrow())
        assertEquals("http://127.0.0.1:5173/stamp.png%3Fv=2", archiveImage("stamp.png?v=2").getOrThrow())
        assertEquals("http://127.0.0.1:5173/x/%252e%252e/stamp.png", archiveImage("x/%2e%2e/stamp.png").getOrThrow())
    }

    /** A phone with no cable reaches the developer's machine through an HTTPS tunnel. */
    @Test
    fun `an archive image keeps the scheme and host of a tunnelled face`() = runTest {
        assertEquals(
            "https://abc.trycloudflare.com/images/stamp.png",
            archiveImage("images/stamp.png", "https://abc.trycloudflare.com/pocket/loyalty.json").getOrThrow(),
        )
    }

    /** The published archive refuses a path that climbs out of it. The preview must not draw what a card cannot. */
    @Test
    fun `an archive path that climbs above the root is refused`() = runTest {
        assertTrue(archiveImage("../secret.png").isFailure)
        assertTrue(archiveImage("images/../../secret.png").isFailure)
    }

    /**
     * A face can come from anyone a tester is persuaded to trust. An archive path must never point at another
     * server, a device on the tester's network, or a file on the phone.
     */
    @Test
    fun `an archive path never leaves the face's server`() = runTest {
        listOf(
            "https://example.com/stamp.png",
            "//example.com/stamp.png",
            "http://192.168.1.1/stamp.png",
            "file:///data/data/io.parity.polkadotapp/databases/app_v2.db",
        ).forEach { path ->
            val resolved = archiveImage(path)
            assertTrue(
                "'$path' resolved to ${resolved.getOrNull()}",
                resolved.isFailure || resolved.getOrThrow().startsWith("http://127.0.0.1:5173/"),
            )
        }
    }

    /**
     * Images are cached by address. A Bulletin image drawn from the developer's server could plant a picture
     * under an address real cards later trust, so a CID always goes to the gateway.
     */
    @Test
    fun `a bulletin image is read from the gateway, never from the face's server`() = runTest {
        assertEquals(GATEWAY_LINK, interactor.resolveImage(FACE_URL, JsImageSource.Bulletin("bafy")).getOrThrow())
    }
}
