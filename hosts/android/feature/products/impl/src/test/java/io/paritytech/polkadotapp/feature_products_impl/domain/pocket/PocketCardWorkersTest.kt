package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardDefinition
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardPreview
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.NativeWorkerClaim
import uniffi.truapi.ProductId

@OptIn(ExperimentalCoroutinesApi::class)
class PocketCardWorkersTest {
    private val key = cardKey(gameProduct, "loyalty")

    private fun publishing(back: PocketCardPreview.Archive? = PocketCardPreview.Archive("back.json")) = ResolvedProduct(
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
                    PocketCardDefinition(
                        PocketCardId("loyalty"), "Loyalty", PocketCardPreview.Archive("face.json"), faceShown = true, back = back,
                    ),
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

    private fun workers(resolver: ResolveProductUseCase) = RealPocketCardWorkers(PublishedPocketCards(resolver), runtimeProvider)

    // A read that did not land, typically a device briefly offline, says nothing about the card.
    // Giving up on it leaves the card static for as long as it stays on screen.
    @Test
    fun `a card whose product could not be read is looked up again`() = runTest {
        val resolver = ScriptedResolver(listOf(Result.failure(ProductResolutionError.Unknown), Result.success(publishing())))

        workers(resolver).holdVisibleSides(key, flowOf(false))

        assertEquals(2, resolver.calls)
        coVerify(exactly = 1) { runtimeProvider.runtime() }
    }

    // A malformed manifest reads the same bytes on every retry, and taking the worker reference
    // for it would boot a worker that can serve no face.
    @Test
    fun `a card whose manifest is malformed is not streamed and not looked up again`() = runTest {
        val resolver = ScriptedResolver(listOf(Result.failure(ProductResolutionError.MalformedManifest)))

        workers(resolver).holdVisibleSides(key, flowOf(false))

        assertEquals(1, resolver.calls)
        coVerify(exactly = 0) { runtimeProvider.runtime() }
    }

    @Test
    fun `each visible side releases its own claim when that side leaves`() = runTest {
        for (declaredBack in listOf(null, PocketCardPreview.Archive("back.json"))) {
            val front = mockk<NativeWorkerClaim>(relaxed = true)
            val back = mockk<NativeWorkerClaim>(relaxed = true)
            val runtime = mockk<TrUAPIHostRuntime> { every { claimWorker(gameProduct) } returnsMany listOf(front, back) }
            coEvery { runtimeProvider.runtime() } returns Result.success(runtime)
            val expanded = MutableStateFlow(false)
            val eligible = workers(ScriptedResolver(listOf(Result.success(publishing(declaredBack)))))
            val visible = launch { eligible.holdVisibleSides(key, expanded) }
            runCurrent()
            verify(exactly = 1) { runtime.claimWorker(gameProduct) }

            val backCount = if (declaredBack == null) 0 else 1
            expanded.value = true
            runCurrent()
            verify(exactly = 1 + backCount) { runtime.claimWorker(gameProduct) }
            expanded.value = false
            runCurrent()
            verify(exactly = backCount) { back.release() }
            verify(exactly = 0) { front.release() }

            visible.cancelAndJoin()
            verify(exactly = 1) { front.release() }
            verify(exactly = backCount) { back.release() }
        }
    }
}
