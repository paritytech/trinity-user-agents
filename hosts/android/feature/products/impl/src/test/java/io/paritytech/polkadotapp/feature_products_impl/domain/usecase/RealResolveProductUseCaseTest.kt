package io.paritytech.polkadotapp.feature_products_impl.domain.usecase

import com.google.gson.Gson
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardPreview
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.bundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCard
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import org.mockito.Mockito.verifyNoInteractions
import uniffi.truapi.ProductId

private const val CID = "QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco"

class RealResolveProductUseCaseTest {
    private val dotNsResolver: DotNsResolver = mock()
    private val productRepository: ProductRepository = mock()
    private val manifestParser = ManifestParser(Gson())

    private val base = "coinflip.dot"
    private val productId = ProductId.parse(base).getOrThrow()

    private var debugCard: DebugPocketCard? = null
    private val debugPocketCards = object : DebugPocketCards {
        override fun get(productId: ProductId): DebugPocketCard? = debugCard
        override fun set(productId: ProductId, card: DebugPocketCard?) = Unit
        override fun appUrl(productId: ProductId): String? = null
        override fun setAppUrl(productId: ProductId, appUrl: String?) = Unit
    }

    private fun useCase(bundled: Map<String, String> = emptyMap()) = RealResolveProductUseCase(
        dotNsResolver,
        manifestParser,
        productRepository,
        debugPocketCards,
        bundledProducts(bundled),
    )

    // A product the app ships resolves from its files: its root manifest names it, and dotNS is not asked.
    @Test
    fun `a bundled product resolves from the files the app ships`() = runBlocking {
        val resolved = useCase(
            mapOf(
                "bundled-products/$base/manifest.json" to rootJson(),
                "bundled-products/$base/worker-manifest.json" to workerJson(),
                "bundled-products/$base/worker/index.js" to "export {}",
            ),
        ).resolve(productId).getOrThrow()

        assertEquals("Coinflip", resolved.product.name)
        assertEquals("https://worker.$base/index.js", resolved.executables.worker?.scriptUrl)
        assertTrue(resolved.executables.app == null)
        verifyNoInteractions(dotNsResolver)
    }

    @Test
    fun `a bundled Worker manifest the parser rejects fails the resolve`() = runBlocking {
        val resolved = useCase(
            mapOf(
                "bundled-products/$base/worker-manifest.json" to """{"${'$'}v":1,"kind":"worker"}""",
                "bundled-products/$base/worker/index.js" to "export {}",
            ),
        ).resolve(productId)

        assertTrue(resolved.isFailure)
    }

    private fun appJson() = """{"${'$'}v":1,"kind":"app","appVersion":[1,0,0]}"""
    private fun widgetJson() = """{"${'$'}v":1,"kind":"widget","appVersion":[1,0,0],"dimensions":{"height":[1,2]}}"""
    private fun workerJson() =
        """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":{"chat":true,"pocket":false}}"""

    private fun rootJson() = """{"${'$'}v":1,"displayName":"Coinflip","description":"d","icon":{"cid":"$CID","format":"png"}}"""

    @Test
    fun `resolves manifest product with all executables`() = runBlocking {
        whenever(dotNsResolver.getMetadataEntry(base, "manifest")).thenReturn(Result.success(rootJson()))
        whenever(dotNsResolver.getMetadataEntry("app.$base", "executable")).thenReturn(Result.success(appJson()))
        whenever(dotNsResolver.getMetadataEntry("widget.$base", "executable")).thenReturn(Result.success(widgetJson()))
        whenever(dotNsResolver.getMetadataEntry("worker.$base", "executable")).thenReturn(Result.success(workerJson()))

        val resolved = useCase().resolve(productId).getOrThrow()

        assertEquals("Coinflip", resolved.product.name)
        assertEquals(CID, resolved.product.icon?.cid?.toString())
        assertEquals(base, resolved.product.id.value)
        assertTrue(resolved.executables.app != null)
        assertTrue(resolved.executables.widget != null)
        assertTrue(resolved.executables.worker != null)
    }

    @Test
    fun `legacy product synthesizes only app`() = runBlocking {
        // No manifest record → legacy product: an APP at the base name, and no worker.
        whenever(dotNsResolver.getMetadataEntry(base, "manifest")).thenReturn(Result.success(null))

        val resolved = useCase().resolve(productId).getOrThrow()

        assertEquals(base, resolved.product.name)
        assertEquals(base, resolved.executables.app?.host?.value)
        assertEquals(null, resolved.executables.widget)
        assertEquals(null, resolved.executables.worker)
    }

    @Test
    fun `surfaces failure when manifest read fails`() = runBlocking {
        whenever(dotNsResolver.getMetadataEntry(base, "manifest"))
            .thenReturn(Result.failure(RuntimeException("registry down")))

        val result = useCase().resolve(productId)

        assertEquals(ProductResolutionError.Unknown, result.exceptionOrNull())
    }

    @Test
    fun `a failed executable read is retryable, not a product without that surface`() = runBlocking {
        whenever(dotNsResolver.getMetadataEntry(base, "manifest")).thenReturn(Result.success(rootJson()))
        whenever(dotNsResolver.getMetadataEntry("app.$base", "executable"))
            .thenReturn(Result.failure(RuntimeException("rpc down")))
        whenever(dotNsResolver.getMetadataEntry("widget.$base", "executable")).thenReturn(Result.success(null))
        whenever(dotNsResolver.getMetadataEntry("worker.$base", "executable")).thenReturn(Result.success(null))

        // Otherwise the miss would cache as "this product has no app" until the process restarts.
        assertTrue(useCase().resolve(productId).isFailure)
    }

    @Test
    fun `a failed resolution is not cached`() = runBlocking {
        whenever(dotNsResolver.getMetadataEntry(base, "manifest"))
            .thenReturn(Result.failure(RuntimeException("registry down")), Result.success(null))
        val useCase = useCase()

        assertTrue(useCase.resolve(productId).isFailure)
        assertTrue(useCase.resolve(productId).isSuccess)
    }

    @Test
    fun `a successful resolution is served from cache`() = runBlocking<Unit> {
        whenever(dotNsResolver.getMetadataEntry(base, "manifest")).thenReturn(Result.success(null))
        val useCase = useCase()

        useCase.resolve(productId).getOrThrow()
        useCase.resolve(productId).getOrThrow()

        verify(dotNsResolver, times(1)).getMetadataEntry(base, "manifest")
    }

    @Test
    fun `surfaces failure when manifest record is malformed`() = runBlocking {
        whenever(dotNsResolver.getMetadataEntry(base, "manifest")).thenReturn(Result.success("not json"))

        val result = useCase().resolve(productId)

        assertEquals(ProductResolutionError.MalformedManifest, result.exceptionOrNull())
    }

    /**
     * A worker served from a developer's machine is the whole point of the debug URL: without a card
     * on it there is nothing to add to the Pocket, and trying a card still costs a publish.
     */
    @Test
    fun `a debug worker carries the card the debug menu names`() = runBlocking {
        debugCard = DebugPocketCard(PocketCardId("loyalty"), "Loyalty", DEV_FACE, faceShown = true)
        whenever(dotNsResolver.getMetadataEntry(base, "manifest")).thenReturn(Result.success(null))
        whenever(productRepository.getUserWorkerUrl(productId)).thenReturn(DEV_WORKER)

        val worker = useCase().resolve(productId).getOrThrow().executables.worker

        assertEquals(true, worker?.includesPocket)
        assertEquals(PocketCardId("loyalty"), worker?.pocketCards?.single()?.id)
        assertEquals(PocketCardPreview.Url(DEV_FACE), worker?.pocketCards?.single()?.preview)
    }

    /** Naming no card must leave the worker exactly as it was before cards existed. */
    @Test
    fun `a debug worker with no card named publishes none`() = runBlocking {
        debugCard = null
        whenever(dotNsResolver.getMetadataEntry(base, "manifest")).thenReturn(Result.success(null))
        whenever(productRepository.getUserWorkerUrl(productId)).thenReturn(DEV_WORKER)

        val worker = useCase().resolve(productId).getOrThrow().executables.worker

        assertEquals(false, worker?.includesPocket)
        assertEquals(emptyList<Any>(), worker?.pocketCards)
    }

    /** A published worker is the product's own word; a local override must not quietly beat it. */
    @Test
    fun `a published worker still wins over the debug one`() = runBlocking {
        debugCard = DebugPocketCard(PocketCardId("loyalty"), "Loyalty", DEV_FACE, faceShown = true)
        whenever(dotNsResolver.getMetadataEntry(base, "manifest")).thenReturn(Result.success(rootJson()))
        whenever(dotNsResolver.getMetadataEntry("app.$base", "executable")).thenReturn(Result.success(appJson()))
        whenever(dotNsResolver.getMetadataEntry("widget.$base", "executable")).thenReturn(Result.success(null))
        whenever(dotNsResolver.getMetadataEntry("worker.$base", "executable")).thenReturn(Result.success(workerJson()))
        whenever(productRepository.getUserWorkerUrl(productId)).thenReturn(DEV_WORKER)

        val worker = useCase().resolve(productId).getOrThrow().executables.worker as ProductExecutable.Worker

        assertEquals("https://worker.coinflip.dot/index.js", worker.scriptUrl)
        assertEquals(emptyList<Any>(), worker.pocketCards)
    }

    private companion object {
        const val DEV_WORKER = "http://127.0.0.1:5173/worker.js"
        const val DEV_FACE = "http://127.0.0.1:5173/faces/loyalty.json"
    }
}
