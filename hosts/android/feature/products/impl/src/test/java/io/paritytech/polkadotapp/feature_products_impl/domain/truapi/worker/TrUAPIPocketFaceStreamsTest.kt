package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardDefinition
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardPreview
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PublishedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.cardKey
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.faceOf
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.gameProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.PocketCardSide
import uniffi.truapi.ProductId
import uniffi.truapi.RenderContext

class TrUAPIPocketFaceStreamsTest {
    private val key = cardKey(gameProduct, "loyalty")

    private fun publishing(back: PocketCardPreview? = null) = ResolvedProduct(
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

    private class ScriptedResolver(private val answers: List<Result<ResolvedProduct>>) : ResolveProductUseCase {
        var calls = 0

        override suspend fun resolve(productId: ProductId): Result<ResolvedProduct> = answers[minOf(calls++, answers.lastIndex)]

        override suspend fun invalidate(productId: ProductId) = Unit
    }

    // Reaching the runtime is the point past the lookup; refusing it ends the stream there.
    private val runtimeProvider = mockk<TrUAPIHostRuntimeProvider> {
        coEvery { runtime() } returns Result.failure(IllegalStateException("runtime unavailable"))
    }

    private fun streams(resolver: ResolveProductUseCase) =
        TrUAPIPocketFaceStreams(runtimeProvider, mockk(relaxed = true), PublishedPocketCards(resolver))

    // A read that did not land, typically a device briefly offline, says nothing about the card.
    // Giving up on it leaves the card static for as long as it stays on screen.
    @Test
    fun `a card whose product could not be read is looked up again`() = runTest {
        val resolver = ScriptedResolver(listOf(Result.failure(ProductResolutionError.Unknown), Result.success(publishing())))

        streams(resolver).renderFaces(key).toList()

        assertEquals(2, resolver.calls)
        coVerify(exactly = 1) { runtimeProvider.runtime() }
    }

    // A malformed manifest reads the same bytes on every retry, and taking the worker reference
    // for it would boot a worker that can serve no face.
    @Test
    fun `a card whose manifest is malformed is not streamed and not looked up again`() = runTest {
        val resolver = ScriptedResolver(listOf(Result.failure(ProductResolutionError.MalformedManifest)))

        streams(resolver).renderFaces(key).toList()

        assertEquals(1, resolver.calls)
        coVerify(exactly = 0) { runtimeProvider.runtime() }
    }

    private val execution = mockk<TrUAPIProductExecution>(relaxed = true) {
        every { render(any()) } returns flowOf(faceOf("drawn"))
    }

    private fun liveStreams(published: ResolvedProduct): TrUAPIPocketFaceStreams {
        val runtime = mockk<TrUAPIHostRuntime>(relaxed = true)
        val workers = mockk<TrUAPIWorkerSupervisor> {
            every { execution(gameProduct) } returns flowOf(execution)
            every { currentExecution(gameProduct) } returns execution
        }
        val provider = mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.success(runtime) }
        return TrUAPIPocketFaceStreams(provider, workers, PublishedPocketCards(ScriptedResolver(listOf(Result.success(published)))))
    }

    // The context is all a product has to tell which side it is asked to draw, or which side was pressed.
    @Test
    fun `the back is drawn and pressed under its own context`() = runTest {
        val streams = liveStreams(publishing(back = PocketCardPreview.Archive("back.json")))

        val drawn = streams.renderFaces(key, PocketCardSide.BACK).toList()
        streams.sendAction(key, PocketCardSide.BACK, "stamp-back", ByteArray(0))

        assertEquals(listOf(faceOf("drawn")), drawn)
        verify { execution.render(match { it.context == RenderContext.PocketCardBack("loyalty") }) }
        verify { execution.publishRendererAction(match { it.context == RenderContext.PocketCardBack("loyalty") }) }
    }

    // A product that declares no back may run an SDK that cannot decode the back's context at all.
    @Test
    fun `a card published without a back is never asked to draw one`() = runTest {
        liveStreams(publishing()).renderFaces(key, PocketCardSide.BACK).toList()

        verify(exactly = 0) { execution.render(any()) }
    }
}
