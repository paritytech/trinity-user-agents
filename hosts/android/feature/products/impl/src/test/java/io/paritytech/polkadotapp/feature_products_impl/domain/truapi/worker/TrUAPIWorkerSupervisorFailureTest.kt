package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import dagger.Lazy
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductHasNoWorker
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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import uniffi.truapi.ProductId

class TrUAPIWorkerSupervisorFailureTest {
    private val productId = ProductId.parse("chat.dot").getOrThrow()
    private val bootEvents = mutableListOf<HostEvent>()

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
        navigation = mock(),
        bootTelemetry = HostWorkerBootTelemetry(
            originProvider = { HostEventOrigin("nightly", "gp", "1.2.3", "android", "15", 123L) },
            timeProvider = { 0L },
            capture = { bootEvents += it() },
        ),
        dispatchers = testDispatchers(),
    )

    private class GatedScriptResolver(private val gate: CompletableDeferred<Unit>) : ProductScriptResolver {
        override suspend fun resolveWorker(productId: ProductId): Result<ProductExecutable.Worker> {
            gate.await()
            error("the gate never completes in this test")
        }
    }

    @Test
    fun `a boot that fails is reported as an error under the activation's session`() = runTest {
        val supervisor = supervisorWith(FailingScriptResolver(IllegalStateException("boom")))

        supervisor.onDemandChanged(productId, WorkerDemand.START)
        advanceUntilIdle()

        val event = bootEvents.single()
        assertEquals("host.worker.boot" to "error", event.name to event.tags["outcome"])
        assertNotNull(event.tags["host.session_id"])
    }

    @Test
    fun `a product with no worker is reported as no worker`() = runTest {
        val supervisor = supervisorWith(FailingScriptResolver(ProductHasNoWorker(productId)))

        supervisor.onDemandChanged(productId, WorkerDemand.START)
        advanceUntilIdle()

        assertEquals(listOf("no_worker"), bootEvents.map { it.tags["outcome"] })
    }

    // Without it, a boot the core gave up on is missing from the data rather than counted as cut short.
    @Test
    fun `a worker stopped mid-boot is reported as abandoned`() = runTest {
        val supervisor = supervisorWith(GatedScriptResolver(CompletableDeferred()))

        supervisor.onDemandChanged(productId, WorkerDemand.START)
        advanceUntilIdle()
        supervisor.onDemandChanged(productId, WorkerDemand.STOP)
        advanceUntilIdle()

        assertEquals(listOf("abandoned"), bootEvents.map { it.tags["outcome"] })
    }

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
