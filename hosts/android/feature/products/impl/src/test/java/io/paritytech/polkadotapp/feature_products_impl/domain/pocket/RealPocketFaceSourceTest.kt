package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.net.Uri
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.mockk.verifyOrder
import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFace
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.MaterialsProps
import uniffi.truapi.ProductId
import uniffi.truapi.RendererNode

class RealPocketFaceSourceTest {
    private class FakeStreams(
        private val resolveDrawn: (RendererNode) -> MaterialsResolution?,
        private val resolveKept: (RendererNode) -> MaterialsResolution?,
    ) : PocketFaceStreams {
        val faces = MutableSharedFlow<RendererNode>()
        var collectors = 0
        val actions = mutableListOf<Triple<PocketCardKey, String, String>>()

        override fun renderFaces(key: PocketCardKey): Flow<RenderedFace> =
            faces.onStart { collectors++ }.map { tree -> RenderedFace(tree) { resolveDrawn(tree) } }

        override fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray) {
            actions += Triple(key, actionId, payload.decodeToString())
        }

        override suspend fun resolveKeptMaterials(key: PocketCardKey, tree: RendererNode): MaterialsResolution? = resolveKept(tree)
    }

    private class NoImages : ProductImageResolver {
        override suspend fun resolve(productId: ProductId, source: JsImageSource): Result<Uri> =
            Result.failure(IllegalStateException("unused"))
    }

    private val loyalty = addedCard(gameProduct, "loyalty")
    private val store = RealPocketCollection(FakePinnedPocketCards(emptyList()), InMemoryPocketCardRepository())
    private val resolveDrawn = mockk<(RendererNode) -> MaterialsResolution?>(relaxed = true)
    private val resolveKept = mockk<(RendererNode) -> MaterialsResolution?> { every { this@mockk(any()) } returns mockk(relaxed = true) }
    private val streams = FakeStreams(resolveDrawn, resolveKept)
    private val source = RealPocketFaceSource(store, streams, NoImages())

    private fun materialsFace(text: String) =
        RendererNode.Materials(MaterialsProps(emptyList()), listOf(RendererNode.String(text)))

    @Test
    fun `shows the kept face first, then every live face`() = runTest {
        store.addCard(loyalty)
        val shown = mutableListOf<PocketFace>()

        val onScreen = source.observeFace(loyalty.card.key).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        streams.faces.emit(faceOf("live 1"))
        streams.faces.emit(faceOf("live 2"))
        advanceUntilIdle()

        assertEquals(listOf(loyalty.face, faceOf("live 1"), faceOf("live 2")).map { it.toJsWidget() }, shown.map { it.widget })
        assertEquals("the live stream is opened once per face on screen", 1, streams.collectors)
        onScreen.cancel()
    }

    @Test
    fun `a card with no cached face still opens the live stream`() = runTest {
        val shown = mutableListOf<PocketFace>()

        val onScreen = source.observeFace(cardKey(gameProduct, "unknown")).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        streams.faces.emit(faceOf("first"))
        advanceUntilIdle()

        assertEquals(listOf(faceOf("first").toJsWidget()), shown.map { it.widget })
        onScreen.cancel()
    }

    @Test
    fun `actions go to the product with the card's key`() {
        source.sendAction(loyalty.card.key, "stamp", "value".toByteArray())

        assertEquals(listOf(Triple(loyalty.card.key, "stamp", "value")), streams.actions)
    }

    // The core asks for the next tree's resolution to start before the old one closes, so the
    // materials both trees share keep their fetch; and nothing may stay open once the card is gone.
    @Test
    fun `each materials face gets one resolution, opened before the last closes and closed when the card goes`() = runTest {
        val first = mockk<MaterialsResolution>(relaxed = true)
        val second = mockk<MaterialsResolution>(relaxed = true)
        every { resolveDrawn(materialsFace("1")) } returns first
        every { resolveDrawn(materialsFace("2")) } returns second
        val shown = mutableListOf<PocketFace>()

        val onScreen = source.observeFace(loyalty.card.key).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        streams.faces.emit(materialsFace("1"))
        streams.faces.emit(materialsFace("2"))
        advanceUntilIdle()
        onScreen.cancel()
        advanceUntilIdle()

        assertEquals(listOf(first, second), shown.map { it.materials })
        verifyOrder {
            resolveDrawn(materialsFace("2"))
            first.close()
        }
        verify(exactly = 1) { first.close() }
        verify(exactly = 1) { second.close() }
    }

    // A kept face was not drawn by the running execution, so its fallbacks must not reach the product.
    @Test
    fun `a kept face resolves on the runtime and a live face on the execution that drew it`() = runTest {
        store.addCard(loyalty.copy(face = materialsFace("kept")))

        val onScreen = source.observeFace(loyalty.card.key).launchIn(this)
        advanceUntilIdle()
        streams.faces.emit(materialsFace("live"))
        advanceUntilIdle()

        coVerifyOrder {
            resolveKept(materialsFace("kept"))
            resolveDrawn(materialsFace("live"))
        }
        verify(exactly = 0) { resolveKept(materialsFace("live")) }
        verify(exactly = 0) { resolveDrawn(materialsFace("kept")) }
        onScreen.cancel()
    }
}
