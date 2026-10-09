package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import dagger.Lazy
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIBootstrapInstaller
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIChainDirectory
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class TrUAPIWorkerSupervisorFailureTest {
    private val productId = ProductId.fromStoredValue("chat.dot")

    private class FailingScriptResolver(private val error: Throwable) : ProductScriptResolver {
        override suspend fun resolveWorker(productId: ProductId): Result<ProductExecutable.Worker> =
            Result.failure(error)
    }

    private class FailOnceThenGateScriptResolver(
        private val error: Throwable,
        private val gate: CompletableDeferred<Unit>,
    ) : ProductScriptResolver {
        private var calls = 0

        override suspend fun resolveWorker(productId: ProductId): Result<ProductExecutable.Worker> {
            calls++
            if (calls == 1) return Result.failure(error)
            gate.await()
            error("the gate never completes in this test")
        }
    }

    private fun TestScope.supervisorWith(
        scriptResolver: ProductScriptResolver,
        runtimeProvider: TrUAPIHostRuntimeProvider = mock(),
        hostBridgeFactory: ProductTrUAPIHostBridge.Factory = mock(),
        webViewProviderFactory: ChatWebViewProvider.Factory = mock(),
        bootstrapInstaller: TrUAPIBootstrapInstaller = mock(),
        refCounter: ProductWorkerRefCounter = mock(),
    ): TrUAPIWorkerSupervisor = TrUAPIWorkerSupervisor(
        runtimeProvider = Lazy { runtimeProvider },
        hostBridgeFactory = hostBridgeFactory,
        chainDirectory = mock<TrUAPIChainDirectory>(),
        scriptResolver = scriptResolver,
        webViewProviderFactory = webViewProviderFactory,
        bootstrapInstaller = bootstrapInstaller,
        refCounter = Lazy { refCounter },
        dispatchers = testDispatchers(),
    )

    @Test
    fun `a boot that fails is reported as failed, not as still pending`() = runTest {
        val boom = IllegalStateException("boom")
        val supervisor = supervisorWith(FailingScriptResolver(boom))

        supervisor.onDemandChanged(productId, WorkerDemand.START)
        advanceUntilIdle()

        val state = supervisor.executionState(productId).filterNotNull().first()
        assertTrue(state is WorkerExecutionState.Failed)
        assertSame(boom, (state as WorkerExecutionState.Failed).reason)
    }

    @Test
    fun `a STOP after the worker was already removed by the boot failure still clears the stale Failed`() = runTest {
        val boom = IllegalStateException("boom")
        val supervisor = supervisorWith(FailingScriptResolver(boom))

        supervisor.onDemandChanged(productId, WorkerDemand.START)
        advanceUntilIdle()
        assertTrue(supervisor.executionState(productId).first() is WorkerExecutionState.Failed)

        supervisor.onDemandChanged(productId, WorkerDemand.STOP)
        advanceUntilIdle()

        assertNull(
            "a STOP must clear the stale Failed even when the worker was already gone from the map",
            supervisor.executionState(productId).first(),
        )
    }

    @Test
    fun `a retry after a failed boot reports pending, not the stale failure`() = runTest {
        val boom = IllegalStateException("boom")
        val gate = CompletableDeferred<Unit>()
        val supervisor = supervisorWith(FailOnceThenGateScriptResolver(boom, gate))

        supervisor.onDemandChanged(productId, WorkerDemand.START)
        advanceUntilIdle()
        assertTrue(supervisor.executionState(productId).first() is WorkerExecutionState.Failed)

        supervisor.onDemandChanged(productId, WorkerDemand.START)
        advanceUntilIdle()

        assertNull(supervisor.executionState(productId).first())
    }
}
