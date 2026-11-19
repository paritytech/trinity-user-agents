package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.mockk.verifyOrder
import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFace
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
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
import uniffi.truapi.RendererNode

class RealPocketFaceSourceTest {
    private val loyalty = addedCard(gameProduct, "loyalty")
    private val store = RealPocketCollection(FakePinnedPocketCards(emptyList()), InMemoryPocketCardRepository())
    private val faces = MutableSharedFlow<RendererNode>()
    private var collectors = 0
    private val resolveDrawn = mockk<(RendererNode) -> MaterialsResolution?>(relaxed = true)
    private val streams = mockk<PocketFaceStreams>(relaxed = true) {
        every { renderFaces(any()) } returns faces
            .onStart { collectors++ }
            .map { tree -> RenderedFace(tree) { resolveDrawn(tree) } }
    }
    private val source = RealPocketFaceSource(store, streams, mockk())

    private fun materialsFace(text: String) =
        RendererNode.Materials(MaterialsProps(emptyList()), listOf(RendererNode.String(text)))

    @Test
    fun `shows the kept face first, then every live face`() = runTest {
        store.addCard(loyalty)
        val shown = mutableListOf<PocketFace>()

        val onScreen = source.observeFace(loyalty.card.key).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        faces.emit(faceOf("live 1"))
        faces.emit(faceOf("live 2"))
        advanceUntilIdle()

        assertEquals(listOf(loyalty.face, faceOf("live 1"), faceOf("live 2")).map { it.toJsWidget() }, shown.map { it.widget })
        assertEquals("the live stream is opened once per face on screen", 1, collectors)
        onScreen.cancel()
    }

    @Test
    fun `a card with no cached face still opens the live stream`() = runTest {
        val shown = mutableListOf<PocketFace>()

        val onScreen = source.observeFace(cardKey(gameProduct, "unknown")).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        faces.emit(faceOf("first"))
        advanceUntilIdle()

        assertEquals(listOf(PocketFace(faceOf("first").toJsWidget(), materials = null)), shown)
        onScreen.cancel()
    }

    @Test
    fun `actions go to the product with the card's key`() {
        val payload = "value".toByteArray()

        source.sendAction(loyalty.card.key, "stamp", payload)

        verify { streams.sendAction(loyalty.card.key, "stamp", payload) }
    }

    // A face without Materials nodes costs the core nothing: no resolver is built for it.
    @Test
    fun `a face without materials nodes is not resolved`() = runTest {
        val onScreen = source.observeFace(loyalty.card.key).launchIn(this)
        advanceUntilIdle()
        faces.emit(faceOf("plain"))
        advanceUntilIdle()

        coVerify(exactly = 0) { streams.resolveKeptMaterials(any(), any()) }
        verify(exactly = 0) { resolveDrawn(any()) }
        onScreen.cancel()
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
        faces.emit(materialsFace("1"))
        faces.emit(materialsFace("2"))
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

    // Neither the execution nor the runtime could resolve: the face still draws, plain.
    @Test
    fun `a face whose materials cannot be resolved still shows`() = runTest {
        every { resolveDrawn(any()) } returns null
        val shown = mutableListOf<PocketFace>()

        val onScreen = source.observeFace(loyalty.card.key).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        faces.emit(materialsFace("1"))
        advanceUntilIdle()

        assertEquals(listOf(PocketFace(materialsFace("1").toJsWidget(), materials = null)), shown)
        onScreen.cancel()
    }

    // A kept face was not drawn by the running execution, so its fallbacks must not reach the product.
    @Test
    fun `a kept face resolves on the runtime and a live face on the execution that drew it`() = runTest {
        store.addCard(loyalty.copy(face = materialsFace("kept")))

        val onScreen = source.observeFace(loyalty.card.key).launchIn(this)
        advanceUntilIdle()
        faces.emit(materialsFace("live"))
        advanceUntilIdle()

        coVerifyOrder {
            streams.resolveKeptMaterials(loyalty.card.key, materialsFace("kept"))
            resolveDrawn(materialsFace("live"))
        }
        coVerify(exactly = 0) { streams.resolveKeptMaterials(loyalty.card.key, materialsFace("live")) }
        verify(exactly = 0) { resolveDrawn(materialsFace("kept")) }
        onScreen.cancel()
    }
}
