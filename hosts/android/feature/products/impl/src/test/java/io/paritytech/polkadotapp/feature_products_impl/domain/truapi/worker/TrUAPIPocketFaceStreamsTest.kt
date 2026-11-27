package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
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
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.plus
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.MaterialsProps
import uniffi.truapi.NativeWorkerClaim
import uniffi.truapi.ProductId
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
                includesChat = true,
                includesPocket = true,
                pocketCards = listOf(
                    PocketCardDefinition(PocketCardId("loyalty"), "Loyalty", PocketCardPreview.Archive("face.json"), startsCovered = false),
                ),
            ),
        ),
    )

    private class ScriptedResolver(private val answers: List<Result<ResolvedProduct>>) : ResolveProductUseCase {
        var calls = 0

        override suspend fun resolve(productId: ProductId, retryPolicy: NetworkRetryPolicy): Result<ResolvedProduct> =
            answers[minOf(calls++, answers.lastIndex)]

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

    @Test
    fun `reopening a Pocket render preserves its claim independently of Chat`() = runTest {
        val chat = mockk<NativeWorkerClaim>(relaxed = true)
        val pocket = mockk<NativeWorkerClaim>(relaxed = true)
        every { runtime.claimWorker(gameProduct) } returnsMany listOf(chat, pocket)
        val workers = mockk<TrUAPIWorkerSupervisor> { every { executionState(gameProduct) } returns emptyFlow() }
        val chatJob = Job(coroutineContext.job)
        TrUAPIChatWorker(gameProduct, runtime, workers, mockk(), this + chatJob)

        val execution = mockk<TrUAPIProductExecution>()
        val faces = drawnBy(execution)
        val reopened = CompletableDeferred<Unit>()
        every { execution.render(any()) } returnsMany listOf(
            flow { error("render interrupted") },
            flow {
                reopened.complete(Unit)
                awaitCancellation()
            },
        )
        val card = launch { faces.renderFaces(key).collect {} }
        reopened.await()

        verify(exactly = 2) { runtime.claimWorker(gameProduct) }
        verify(exactly = 0) { pocket.release() }
        card.cancelAndJoin()
        verify(exactly = 1) { pocket.release() }
        verify(exactly = 0) { chat.release() }
        chatJob.cancelAndJoin()
        verify(exactly = 1) { chat.release() }
    }

    @Test
    fun `a resolution that throws leaves the face plain`() = runTest {
        val execution = mockk<TrUAPIProductExecution> { every { resolveMaterials(any(), any(), any()) } throws IllegalStateException() }

        assertNull(drawnBy(execution).renderFaces(key).first().resolveMaterials())
    }
}
