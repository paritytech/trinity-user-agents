package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.Called
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.cardKey
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.gameProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.flow.first
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
        return TrUAPIPocketFaceStreams(provider, workers)
    }

    @Test
    fun `drawing on an existing execution leaves worker ownership with the caller`() = runTest {
        val execution = mockk<TrUAPIProductExecution>()

        drawnBy(execution).renderFaces(key).first()

        verify { runtime wasNot Called }
    }

    @Test
    fun `a resolution that throws leaves the face plain`() = runTest {
        val execution = mockk<TrUAPIProductExecution> { every { resolveMaterials(any(), any(), any()) } throws IllegalStateException() }

        assertNull(drawnBy(execution).renderFaces(key).first().resolveMaterials())
    }
}
