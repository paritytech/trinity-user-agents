package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import dagger.Lazy
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIChatWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import uniffi.truapi.ProductId

@OptIn(ExperimentalCoroutinesApi::class)
class RealWorkerBootFactoryTest {
    private val productId = ProductId.parse("chat.dot").getOrThrow()
    private val botApi = BindableProductsBotApi(mock(), mock())

    private fun factory(
        runtimeProvider: TrUAPIHostRuntimeProvider,
        workerSupervisor: TrUAPIWorkerSupervisor = mock(),
    ): RealWorkerBootFactory = RealWorkerBootFactory(
        runtimeProvider = Lazy { runtimeProvider },
        workerSupervisor = Lazy { workerSupervisor },
    )

    @Test
    fun `boots the core worker`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mock()
        val runtime: TrUAPIHostRuntime = mock()
        whenever(runtimeProvider.runtime()).thenReturn(Result.success(runtime))
        val workerSupervisor: TrUAPIWorkerSupervisor = mock()
        whenever(workerSupervisor.executionState(productId)).thenReturn(emptyFlow())
        val factory = factory(runtimeProvider, workerSupervisor)
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler))

        val worker = factory.boot(productId, botApi, scope)
        advanceUntilIdle()

        assertTrue("must boot the core worker", worker is TrUAPIChatWorker)
    }

    @Test
    fun `a construction failure surfaces`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mock()
        val runtime: TrUAPIHostRuntime = mock()
        whenever(runtimeProvider.runtime()).thenReturn(Result.success(runtime))
        whenever(runtime.claimWorker(productId)).thenThrow(IllegalStateException("claimWorker boom"))
        val factory = factory(runtimeProvider)

        val failure = runCatching { factory.boot(productId, botApi, this) }.exceptionOrNull()

        assertTrue("a core boot failure must surface", failure is IllegalStateException)
    }
}
