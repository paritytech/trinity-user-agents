package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.net.Uri
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId
import uniffi.truapi.RendererNode

class RealPocketFaceSourceTest {
    private class FakeStreams : PocketFaceStreams {
        val faces = MutableSharedFlow<RendererNode>()
        var collectors = 0
        var drawnByNoWorker = false
        val actions = mutableListOf<Triple<PocketCardKey, String, String>>()

        override fun renderFaces(key: PocketCardKey, load: PocketFaceLoad): Flow<RendererNode> = faces.onStart {
            collectors++
            if (drawnByNoWorker) load.noWorker()
        }

        override fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray) {
            actions += Triple(key, actionId, payload.decodeToString())
        }
    }

    private class NoImages : ProductImageResolver {
        override suspend fun resolve(productId: ProductId, source: JsImageSource): Result<Uri> =
            Result.failure(IllegalStateException("unused"))
    }

    private val loyalty = addedCard(gameProduct, "loyalty")
    private val store = RealPocketCollection(FakePinnedPocketCards(emptyList()), InMemoryPocketCardRepository())
    private val streams = FakeStreams()
    private val reportedOutcomes = mutableListOf<String?>()
    private val telemetry = HostPocketFaceLoadedTelemetry(
        originProvider = { HostEventOrigin("nightly", "gp", "1.2.3", "android", "15", 123L, "launch-7") },
        timeProvider = { 0L },
        capture = { event -> reportedOutcomes += event().tags["outcome"] },
    )
    private val source = RealPocketFaceSource(store, streams, NoImages(), telemetry)

    @Test
    fun `reports the kept face as a cache hit and the first live face as loaded`() = runTest {
        store.addCard(loyalty)

        // Not advanced until idle: that would run out the face timeout first.
        val onScreen = source.observeFace(loyalty.card.key).launchIn(this)
        runCurrent()
        streams.faces.emit(faceOf("live 1"))
        streams.faces.emit(faceOf("live 2"))
        runCurrent()

        assertEquals(listOf("cache_hit", "ok"), reportedOutcomes)
        onScreen.cancel()
    }

    @Test
    fun `a face the stream says no worker will draw is reported as such`() = runTest {
        streams.drawnByNoWorker = true

        val onScreen = source.observeFace(loyalty.card.key).launchIn(this)
        runCurrent()

        assertEquals(listOf("no_worker"), reportedOutcomes)
        onScreen.cancel()
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
    fun `actions go to the product with the card's key`() {
        source.sendAction(loyalty.card.key, "stamp", "value".toByteArray())

        assertEquals(listOf(Triple(loyalty.card.key, "stamp", "value")), streams.actions)
    }
}
