package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.net.Uri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardDefinition
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardPreview
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.mockito.Mockito.mock
import org.mockito.Mockito.verifyNoInteractions
import uniffi.truapi.RendererNode
import uniffi.truapi.TextProps
import java.io.File

private const val ONE_MEGABYTE = 1024 * 1024
private const val ARCHIVE_PATH = "face.json"

class PocketPreviewLoaderTest {
    @get:Rule
    val workerArchive = TemporaryFolder()

    private val dotNsResolver: DotNsResolver = mock()

    private val fetchedUrls = mutableListOf<String>()
    private var remoteFace: Result<RendererNode> = Result.success(faceOf("from the dev server"))
    private val remoteFaces = object : RemoteFaceSource {
        override suspend fun fetch(url: String): Result<RendererNode> {
            fetchedUrls += url
            return remoteFace
        }
    }

    private val loader = PocketPreviewLoader(
        archive = ProductWorkerArchive(dotNsResolver),
        remoteFaces = remoteFaces,
    )

    private fun definition(preview: PocketCardPreview) =
        PocketCardDefinition(id = PocketCardId("loyalty"), title = "Loyalty", preview = preview)

    /**
     * The preview is read on the way to the add sheet, before the user has approved anything, so how
     * much of it there is to read is entirely the product's choice. It has to be refused on its size
     * alone: this one is well-formed, and decoding it is already too late.
     */
    @Test
    fun `a preview of a megabyte is refused on its size rather than read whole`() = runBlocking {
        publishPreview(textFace("x".repeat(ONE_MEGABYTE)))

        val result = loader.load(gameProduct, definition(PocketCardPreview.Archive(ARCHIVE_PATH)))

        assertTrue("a megabyte of well-formed face still has to be refused", result.isFailure)
    }

    @Test
    fun `a preview of the size a real face is decodes`() = runBlocking {
        publishPreview(textFace("Loyalty"))

        val face = loader.load(gameProduct, definition(PocketCardPreview.Archive(ARCHIVE_PATH))).getOrThrow()

        assertEquals(RendererNode.Text(emptyList(), TextProps(style = null, color = null), listOf(RendererNode.String("Loyalty"))), face)
    }

    /**
     * A worker served from a developer's machine has no archive to read, so its card names a URL
     * instead. Without this the add sheet cannot be reached at all for a locally served worker, and
     * the whole point of the debug worker is that it needs no publish.
     */
    @Test
    fun `a url preview is fetched rather than looked for in the archive`() = runBlocking {
        val face = loader.load(gameProduct, definition(PocketCardPreview.Url(DEV_SERVER_FACE))).getOrThrow()

        assertEquals(listOf(DEV_SERVER_FACE), fetchedUrls)
        assertEquals(faceOf("from the dev server"), face)
    }

    /** The archive is resolved through dotNS, which a locally served worker has no entry in. */
    @Test
    fun `a url preview does not touch dotNS`() = runBlocking {
        loader.load(gameProduct, definition(PocketCardPreview.Url(DEV_SERVER_FACE)))

        verifyNoInteractions(dotNsResolver)
    }

    @Test
    fun `a url preview that cannot be fetched fails rather than falling back to the archive`() = runBlocking {
        remoteFace = Result.failure(IllegalStateException("dev server is not running"))

        val result = loader.load(gameProduct, definition(PocketCardPreview.Url(DEV_SERVER_FACE)))

        assertTrue("an unreachable dev server must surface, not read a stale archive", result.isFailure)
    }

    private fun textFace(text: String) =
        """{"tag":"Text","value":{"modifiers":[],"props":{},"children":[{"tag":"String","value":{"text":"$text"}}]}}"""

    private suspend fun publishPreview(face: String) {
        File(workerArchive.root, ARCHIVE_PATH).writeText(face)

        val archiveUri: Uri = mock()
        whenever(archiveUri.path).thenReturn(workerArchive.root.path)
        whenever(dotNsResolver.resolveToLocalUri("worker.game.dot")).thenReturn(Result.success(archiveUri))
    }

    private companion object {
        const val DEV_SERVER_FACE = "http://127.0.0.1:5173/faces/loyalty.json"
    }
}
