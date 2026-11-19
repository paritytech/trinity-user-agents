package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.MaterialsResolution
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
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.gameProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.truapi.MaterialsProps
import uniffi.truapi.ProductId
import uniffi.truapi.RenderContext
import uniffi.truapi.RendererNode

class TrUAPIPocketFaceStreamsTest {
    private val key = cardKey(gameProduct, "loyalty")

    private val publishing = ResolvedProduct(
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
                    PocketCardDefinition(PocketCardId("loyalty"), "Loyalty", PocketCardPreview.Archive("face.json"), faceShown = true),
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
        val resolver = ScriptedResolver(listOf(Result.failure(ProductResolutionError.Unknown), Result.success(publishing)))

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

    private val tree = RendererNode.Materials(MaterialsProps(emptyList()), emptyList())
    private val cardContext = RenderContext.PocketCard("loyalty")

    private val runtime = mockk<TrUAPIHostRuntime>(relaxed = true)

    // The supervisor has already moved past the execution that drew the tree, as it does while a worker reboots.
    private fun drawnBy(execution: TrUAPIProductExecution): TrUAPIPocketFaceStreams {
        every { execution.render(any()) } returns flowOf(tree)
        val provider = mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.success(runtime) }
        val workers = mockk<TrUAPIWorkerSupervisor> {
            every { execution(gameProduct) } returns flowOf(execution)
            every { currentExecution(gameProduct) } returns null
        }
        val published = PublishedPocketCards(ScriptedResolver(listOf(Result.success(publishing))))
        return TrUAPIPocketFaceStreams(provider, workers, published)
    }

    // On the execution that drew the tree a node that falls back is also reported to the product. Float
    // images are filtered in the shader on every device, so no GPU is asked whether it can.
    @Test
    fun `a drawn tree resolves on the execution that drew it`() = runTest {
        val resolution = mockk<MaterialsResolution>()
        val execution = mockk<TrUAPIProductExecution> { every { resolveMaterials(tree, cardContext, false) } returns resolution }

        val resolved = drawnBy(execution).renderFaces(key).first().resolveMaterials()

        assertSame(resolution, resolved)
        verify(exactly = 0) { runtime.resolveMaterials(any(), any(), any()) }
    }

    // No execution drew a kept face, so the product must not hear of its fallbacks.
    @Test
    fun `a kept face resolves on the runtime`() = runTest {
        val resolution = mockk<MaterialsResolution>()
        val runtime = mockk<TrUAPIHostRuntime> { every { resolveMaterials(tree, cardContext, false) } returns resolution }
        val provider = mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.success(runtime) }

        val resolved = TrUAPIPocketFaceStreams(provider, mockk(), mockk()).resolveKeptMaterials(key, tree)

        assertSame(resolution, resolved)
    }

    @Test
    fun `a resolution that throws leaves the face plain`() = runTest {
        val execution = mockk<TrUAPIProductExecution> { every { resolveMaterials(any(), any(), any()) } throws IllegalStateException() }

        assertNull(drawnBy(execution).renderFaces(key).first().resolveMaterials())
    }
}
