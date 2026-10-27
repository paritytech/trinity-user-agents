package io.paritytech.polkadotapp.feature_products_impl.domain.pocketFacePreview

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RemoteFaceSource
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test

private const val FACE_URL = "http://127.0.0.1:5173/pocket/loyalty.json"

class PocketFacePreviewInteractorTest {
    private val remoteFaces = mockk<RemoteFaceSource> {
        coEvery { fetch(any(), any()) } returns Result.failure(IllegalStateException("unused"))
    }

    private val interactor = PocketFacePreviewInteractor(remoteFaces, mockk())

    private suspend fun archiveImage(path: String) = interactor.resolveImage(FACE_URL, JsImageSource.Archive(path))

    /** Drawing again is how a developer sees an edited face, so the cache must not answer for the server. */
    @Test
    fun `a previewed face is fetched past the cache`() = runTest {
        interactor.loadFace(FACE_URL)

        coVerify { remoteFaces.fetch(FACE_URL, bypassCache = true) }
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
}
