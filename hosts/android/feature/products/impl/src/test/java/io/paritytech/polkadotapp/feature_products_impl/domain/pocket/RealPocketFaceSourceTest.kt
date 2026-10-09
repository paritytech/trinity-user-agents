package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.net.Uri
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.RendererNode

class RealPocketFaceSourceTest {
    private class FakeStreams : PocketFaceStreams {
        val faces = MutableSharedFlow<RendererNode>()
        var collectors = 0
        val actions = mutableListOf<Triple<PocketCardKey, String, String>>()

        override fun renderFaces(key: PocketCardKey): Flow<RendererNode> = faces.onStart { collectors++ }

        override fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray) {
            actions += Triple(key, actionId, payload.decodeToString())
        }
    }

    private class NoImages : PocketImageResolver {
        override suspend fun resolve(productId: ProductId, source: JsImageSource): Result<Uri> =
            Result.failure(IllegalStateException("unused"))
    }

    private val loyalty = addedCard(gameProduct, "loyalty")
    private val store = RealPocketCollection(FakePinnedPocketCards(emptyList()), InMemoryPocketCardRepository())
    private val streams = FakeStreams()
    private val source = RealPocketFaceSource(store, streams, NoImages())

    // The store writes to Room, and the flow it feeds is shared into a ViewModel scope with no
    // handler, so a failing write would take the process down rather than the card's freshness.
    private class FailingStore(private val delegate: PocketCardStore) : PocketCardStore by delegate {
        override suspend fun cacheFace(key: PocketCardKey, face: RendererNode) = throw IllegalStateException("disk full")
    }

    @Test
    fun `shows the cached face first, then every live face, and remembers the newest`() = runTest {
        store.addCard(loyalty)
        val shown = mutableListOf<JsWidget>()

        val onScreen = source.observeFace(loyalty.card.key).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        streams.faces.emit(faceOf("live 1"))
        streams.faces.emit(faceOf("live 2"))
        advanceUntilIdle()

        assertEquals(listOf(loyalty.face, faceOf("live 1"), faceOf("live 2")).map { it.toJsWidget() }, shown)
        assertEquals("the live stream is opened once per face on screen", 1, streams.collectors)
        assertEquals(faceOf("live 2"), store.cachedFace(loyalty.card.key))
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
    fun `actions go to the product with the card's key`() {
        source.sendAction(loyalty.card.key, "stamp", "value".toByteArray())

        assertEquals(listOf(Triple(loyalty.card.key, "stamp", "value")), streams.actions)
    }

    @Test
    fun `a face that cannot be kept is still drawn`() = runTest {
        val shown = mutableListOf<JsWidget>()
        val failing = RealPocketFaceSource(FailingStore(store), streams, NoImages())

        val onScreen = failing.observeFace(loyalty.card.key).onEach { shown += it }.launchIn(this)
        advanceUntilIdle()
        streams.faces.emit(faceOf("live"))
        advanceUntilIdle()

        assertEquals(listOf(faceOf("live").toJsWidget()), shown)
        onScreen.cancel()
    }

    // A face is what the user is waiting for; keeping it is bookkeeping. Drawing only after the row
    // is written puts a SQLite transaction in front of every frame, and puts a face that throws
    // while drawing into the database before anyone finds out.
    @Test
    fun `a face is drawn before it is kept`() = runTest {
        val order = mutableListOf<String>()
        val recording = object : PocketCardStore by store {
            override suspend fun cacheFace(key: PocketCardKey, face: RendererNode) {
                order += "kept"
                store.cacheFace(key, face)
            }
        }

        val onScreen = RealPocketFaceSource(recording, streams, NoImages())
            .observeFace(loyalty.card.key)
            .onEach { order += "drawn" }
            .launchIn(this)
        advanceUntilIdle()
        streams.faces.emit(faceOf("live"))
        advanceUntilIdle()

        assertEquals(listOf("drawn", "kept"), order)
        onScreen.cancel()
    }
}
