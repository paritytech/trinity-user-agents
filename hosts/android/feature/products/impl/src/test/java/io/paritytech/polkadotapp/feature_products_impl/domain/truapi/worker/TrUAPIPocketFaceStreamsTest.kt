package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PublishedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.cardKey
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.gameProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.MaterialsProps
import uniffi.truapi.RendererNode

class TrUAPIPocketFaceStreamsTest {
    private val key = cardKey(gameProduct, "loyalty")

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
        val published = mockk<PublishedPocketCards> { coEvery { awaitPublished(key) } returns mockk() }
        return TrUAPIPocketFaceStreams(provider, workers, published)
    }

    @Test
    fun `a stale card cannot draw on the worker already running for chat`() = runTest {
        val execution = mockk<TrUAPIProductExecution>()
        val workers = mockk<TrUAPIWorkerSupervisor> { every { execution(gameProduct) } returns flowOf(execution) }
        val published = mockk<PublishedPocketCards> { coEvery { awaitPublished(key) } returns null }
        val faces = TrUAPIPocketFaceStreams(mockk(), workers, published)

        assertNull(faces.renderFaces(key).firstOrNull())
        verify(exactly = 0) { execution.render(any()) }
    }

    @Test
    fun `a failed resolution leaves the face plain without claiming the worker`() = runTest {
        val execution = mockk<TrUAPIProductExecution> { every { resolveMaterials(any(), any(), any()) } throws IllegalStateException() }

        assertNull(drawnBy(execution).renderFaces(key).first().resolveMaterials())
        verify(exactly = 0) { runtime.claimWorker(any()) }
    }
}
