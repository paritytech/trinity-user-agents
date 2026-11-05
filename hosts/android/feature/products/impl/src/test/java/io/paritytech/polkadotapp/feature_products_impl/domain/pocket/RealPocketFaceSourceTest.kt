package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.net.Uri
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardDefinition
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardPreview
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.PocketCardSide
import uniffi.truapi.ProductId
import uniffi.truapi.RendererNode

class RealPocketFaceSourceTest {
    private class FakeStreams : PocketFaceStreams {
        val faces = MutableSharedFlow<RendererNode>()
        val backs = MutableSharedFlow<RendererNode>()
        var collectors = 0
        val actions = mutableListOf<Triple<PocketCardKey, String, String>>()

        override fun renderFaces(key: PocketCardKey, side: PocketCardSide): Flow<RendererNode> = when (side) {
            PocketCardSide.FRONT -> faces.onStart { collectors++ }
            PocketCardSide.BACK -> backs
        }

        override fun sendAction(key: PocketCardKey, side: PocketCardSide, actionId: String, payload: ByteArray) {
            actions += Triple(key, "$side $actionId", payload.decodeToString())
        }
    }

    private class AnsweringResolver(private val answer: suspend () -> Result<ResolvedProduct>) : ResolveProductUseCase {
        override suspend fun resolve(productId: ProductId): Result<ResolvedProduct> = answer()

        override suspend fun invalidate(productId: ProductId) = Unit
    }

    private class ShippedBacks : RemoteFaceSource {
        override suspend fun fetch(url: String, bypassCache: Boolean): Result<RendererNode> = Result.success(faceOf("shipped $url"))
    }

    private class NoImages : ProductImageResolver {
        override suspend fun resolve(productId: ProductId, source: JsImageSource): Result<Uri> =
            Result.failure(IllegalStateException("unused"))
    }

    private val loyalty = addedCard(gameProduct, "loyalty")
    private val repository = InMemoryPocketCardRepository()
    private val store = RealPocketCollection(FakePinnedPocketCards(emptyList()), repository)
    private val streams = FakeStreams()
    private var lookup: suspend () -> Result<ResolvedProduct> = { awaitCancellation() }
    private val source = RealPocketFaceSource(
        store,
        streams,
        NoImages(),
        PublishedPocketCards(AnsweringResolver { lookup() }),
        PocketPreviewLoader(ProductWorkerArchive(mockk()), ShippedBacks()),
    )

    private fun publishing(back: PocketCardPreview?) = ResolvedProduct(
        product = Product(id = gameProduct, name = "Game", icon = null),
        description = null,
        executables = Executables(
            app = null,
            widget = null,
            worker = ProductExecutable.Worker(
                scriptUrl = "https://worker.game.dot/index.js",
                appVersion = SemVer.ZERO,
                includesChat = false,
                includesPocket = true,
                pocketCards = listOf(
                    PocketCardDefinition(PocketCardId("loyalty"), "Loyalty", PocketCardPreview.Archive("face.json"), faceShown = true, back = back),
                ),
            ),
        ),
    )

    private fun TestScope.backsShown(): MutableList<JsWidget?> {
        val shown = mutableListOf<JsWidget?>()
        source.observeBack(loyalty.card.key).onEach { shown += it }.launchIn(backgroundScope)
        return shown
    }

    @Test
    fun `shows the kept face first, then every live face`() = runTest {
        store.addCard(loyalty)
        val shown = mutableListOf<JsWidget>()

        val onScreen = source.observeFace(loyalty.card.key).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        streams.faces.emit(faceOf("live 1"))
        streams.faces.emit(faceOf("live 2"))
        advanceUntilIdle()

        assertEquals(listOf(loyalty.face, faceOf("live 1"), faceOf("live 2")).map { it.toJsWidget() }, shown)
        assertEquals("the live stream is opened once per face on screen", 1, streams.collectors)
        onScreen.cancel()
    }

    @Test
    fun `a card with no cached face still opens the live stream`() = runTest {
        val shown = mutableListOf<JsWidget>()

        val onScreen = source.observeFace(cardKey(gameProduct, "unknown")).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        streams.faces.emit(faceOf("first"))
        advanceUntilIdle()

        assertEquals(listOf(faceOf("first").toJsWidget()), shown)
        onScreen.cancel()
    }

    @Test
    fun `actions go to the product with the card's key and side`() {
        source.sendAction(loyalty.card.key, PocketCardSide.FRONT, "stamp", "value".toByteArray())
        source.sendAction(loyalty.card.key, PocketCardSide.BACK, "stamp-back", ByteArray(0))

        assertEquals(
            listOf(Triple(loyalty.card.key, "FRONT stamp", "value"), Triple(loyalty.card.key, "BACK stamp-back", "")),
            streams.actions,
        )
    }

    // A turn before the manifest is read still finds the person's own back, even offline.
    @Test
    fun `a kept back shows before the card is looked up`() = runTest {
        repository.faces[loyalty.card.key to PocketCardSide.BACK] = faceOf("kept back")

        val shown = backsShown()
        runCurrent()

        assertEquals(listOf(faceOf("kept back").toJsWidget()), shown)
    }

    @Test
    fun `with no kept back, the shipped back shows until the product draws one`() = runTest {
        lookup = { Result.success(publishing(back = PocketCardPreview.Url("back.json"))) }

        val shown = backsShown()
        runCurrent()
        streams.backs.emit(faceOf("live back"))
        runCurrent()

        assertEquals(listOf(faceOf("shipped back.json"), faceOf("live back")).map { it.toJsWidget() }, shown)
    }

    // A back the product took away must stop showing, though the core still holds the last one drawn.
    @Test
    fun `a card published without a back has none, whatever was kept`() = runTest {
        repository.faces[loyalty.card.key to PocketCardSide.BACK] = faceOf("kept back")
        lookup = { Result.success(publishing(back = null)) }

        val shown = backsShown()
        runCurrent()

        assertEquals(listOf(faceOf("kept back").toJsWidget(), null), shown)
    }
}
