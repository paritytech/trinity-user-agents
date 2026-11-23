package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.MaterialsResolution
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIWorkerExecution
import io.parity.truapi.WorkerExecutionState
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
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.MaterialsProps
import uniffi.truapi.NativeWorkerClaim
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
        TrUAPIPocketFaceStreams(runtimeProvider, PublishedPocketCards(resolver))

    @OptIn(ExperimentalCoroutinesApi::class)
    @Test
    fun `a pending replacement cancels the old render while Pocket keeps its claim`() = runTest {
        val stopped = CompletableDeferred<Unit>()
        val old = mockk<TrUAPIWorkerExecution> {
            every { render(any()) } returns flow {
                try { awaitCancellation() } finally { stopped.complete(Unit) }
            }
        }
        val replacement = mockk<TrUAPIWorkerExecution> {
            every { render(any()) } returns flow { emit(RendererNode.Nil); awaitCancellation() }
        }
        val states = MutableStateFlow<WorkerExecutionState>(WorkerExecutionState.Running(old))
        val claim = mockk<NativeWorkerClaim>(relaxUnitFun = true)
        val runtime = mockk<TrUAPIHostRuntime> {
            every { claimWorker(key.productId) } returns claim
            every { workerExecutionState(key.productId) } returns states
        }
        coEvery { runtimeProvider.runtime() } returns Result.success(runtime)
        val faces = streams(ScriptedResolver(listOf(Result.success(publishing)))).renderFaces(key)
        val drawn = mutableListOf<RendererNode>()
        val collector = launch { faces.map { it.tree }.toList(drawn) }
        runCurrent()
        states.value = WorkerExecutionState.Pending
        runCurrent()
        assertTrue(stopped.isCompleted)
        verify(exactly = 0) { claim.release() }

        states.value = WorkerExecutionState.Running(replacement)
        runCurrent()
        collector.cancelAndJoin()

        assertEquals(listOf(RendererNode.Nil), drawn)
        verify(exactly = 1) { claim.release() }
    }

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
    private val context = RenderContext.PocketCard(key.cardId.value)

    private fun drawnBy(execution: TrUAPIWorkerExecution): TrUAPIPocketFaceStreams {
        every { execution.render(any()) } returns flowOf(tree)
        every { runtime.workerExecutionState(gameProduct) } returns flowOf(WorkerExecutionState.Running(execution))
        coEvery { runtimeProvider.runtime() } returns Result.success(runtime)
        return streams(ScriptedResolver(listOf(Result.success(publishing))))
    }

    @Test
    fun `only the execution that drew a live face receives its material reports`() = runTest {
        val live = mockk<MaterialsResolution>()
        val kept = mockk<MaterialsResolution>()
        val execution = mockk<TrUAPIWorkerExecution> { every { resolveMaterials(tree, context, false) } returns live }
        every { runtime.resolveMaterials(tree, context, false) } returns kept
        val faces = drawnBy(execution)
        val drawn = faces.renderFaces(key).first()

        assertEquals(live, drawn.resolveMaterials())
        assertEquals(kept, faces.resolveKeptMaterials(key, tree))
        verify(exactly = 0) { runtime.currentWorkerExecution(any()) }
    }

    @Test
    fun `a resolution that throws leaves the face plain`() = runTest {
        val execution = mockk<TrUAPIWorkerExecution> { every { resolveMaterials(any(), any(), any()) } throws IllegalStateException() }

        assertNull(drawnBy(execution).renderFaces(key).first().resolveMaterials())
    }
}
