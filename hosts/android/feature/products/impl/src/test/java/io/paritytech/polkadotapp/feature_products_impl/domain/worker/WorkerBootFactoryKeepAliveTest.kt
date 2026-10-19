package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.WorkerKeepAlive
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class WorkerBootFactoryKeepAliveTest {
    private val productId = ProductId.fromStoredValue("verbosity.dot")
    private val runtime = mockk<TrUAPIHostRuntime>(relaxed = true)
    private val supervisor = mockk<TrUAPIWorkerSupervisor>(relaxed = true) {
        every { executionState(productId) } returns emptyFlow()
    }

    private fun factory(keepAlive: WorkerKeepAlive) = RealWorkerBootFactory(
        scriptExecutorFactory = mockk(),
        runtimeSettings = mockk { every { isTrUAPIRuntimeEnabled() } returns true },
        runtimeProvider = { mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.success(runtime) } },
        workerSupervisor = { supervisor },
        scriptResolver = object : ProductScriptResolver {
            override suspend fun resolveWorker(productId: ProductId) =
                Result.success(mockk<ProductExecutable.Worker> { every { this@mockk.keepAlive } returns keepAlive })
        },
        inAppRuntimes = emptySet(),
    )

    private suspend fun TestScope.bootAndDispose(keepAlive: WorkerKeepAlive) {
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler))
        factory(keepAlive).boot(productId, BindableProductsBotApi(mockk(), mockk()), scope)
        advanceUntilIdle()
        scope.cancel()
        advanceUntilIdle()
    }

    @Test
    fun `a foreground worker is declared while the product is booted`() = runTest {
        bootAndDispose(WorkerKeepAlive.FOREGROUND)

        verify(exactly = 1) { supervisor.declareForegroundWorker(runtime, productId) }
        verify(exactly = 1) { supervisor.withdrawForegroundWorker(runtime, productId) }
    }

    @Test
    fun `an on-demand worker declares nothing`() = runTest {
        bootAndDispose(WorkerKeepAlive.ON_DEMAND)

        verify(exactly = 0) { supervisor.declareForegroundWorker(any(), any()) }
    }
}
