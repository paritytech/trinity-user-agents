package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.ChatHostBridge
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.NativeWasmWorkerEnd
import uniffi.truapi.WorkerKeepAlive

class TrUAPIWorkerSupervisorWasmTest {
    private val executions = mutableListOf<TrUAPIProductExecution>()
    private val ends = mutableListOf<CompletableDeferred<NativeWasmWorkerEnd>>()
    private val bootstraps = mutableListOf<(suspend (String) -> Unit)?>()
    private val chatBridges = mutableListOf<ChatHostBridge?>()
    private var webViews = 0
    private val runtime = mockk<TrUAPIHostRuntime>(relaxed = true)

    @Test
    fun `a wasm worker runs in the core with no WebView and no bridge`() = runTest {
        val supervisor = supervisor()

        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()

        assertSame(executions.single(), supervisor.currentExecution(PRODUCT))
        verify { executions.single().startWasmWorker(MODULE) }
        assertEquals(listOf<(suspend (String) -> Unit)?>(null), bootstraps)
        assertEquals(0, webViews)
        assertEquals(1, chatBridges.filterNotNull().size)
    }

    @Test
    fun `a module that fails ends the worker as a failed start`() = runTest {
        val supervisor = supervisor()
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()

        ends.single().complete(NativeWasmWorkerEnd.Failed("trap: unreachable"))
        advanceUntilIdle()

        val state = supervisor.executionState(PRODUCT).first()
        assertTrue(state is WorkerExecutionState.Failed)
        assertEquals("trap: unreachable", (state as WorkerExecutionState.Failed).reason.message)
    }

    // Nothing serves the product after its entry point returns, so a clean return is a fault too.
    @Test
    fun `a module whose entry point returns ends the worker as a failed start`() = runTest {
        val supervisor = supervisor()
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()

        ends.single().complete(NativeWasmWorkerEnd.Finished)
        advanceUntilIdle()

        assertTrue(supervisor.executionState(PRODUCT).first() is WorkerExecutionState.Failed)
    }

    @Test
    fun `a stopped module is no fault`() = runTest {
        val supervisor = supervisor()
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()

        ends.single().complete(NativeWasmWorkerEnd.Stopped)
        advanceUntilIdle()

        assertSame(executions.single(), supervisor.currentExecution(PRODUCT))
    }

    // The core keeps the references of the open rooms after a fault and sends no new start.
    @Test
    fun `a worker that ended on a fault starts again for a room and in the foreground`() = runTest {
        val supervisor = supervisor()
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()
        ends[0].complete(NativeWasmWorkerEnd.Failed("trap"))
        advanceUntilIdle()

        supervisor.restartIfFailed(PRODUCT)
        advanceUntilIdle()
        ends[1].complete(NativeWasmWorkerEnd.Failed("trap"))
        advanceUntilIdle()
        supervisor.restartFailed()
        advanceUntilIdle()

        assertEquals(3, executions.size)
        assertSame(executions[2], supervisor.currentExecution(PRODUCT))
    }

    @Test
    fun `a stop of the worker clears its state`() = runTest {
        val supervisor = supervisor()
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()

        supervisor.onDemandChanged(PRODUCT, WorkerDemand.STOP)
        advanceUntilIdle()

        assertNull(supervisor.executionState(PRODUCT).first())
    }

    @Test
    fun `the core gets the foreground keep alive at the first declaration and loses it at the last`() = runTest {
        val supervisor = supervisor()

        supervisor.declareForegroundWorker(runtime, PRODUCT)
        supervisor.declareForegroundWorker(runtime, PRODUCT)
        supervisor.withdrawForegroundWorker(runtime, PRODUCT)
        verify(exactly = 1) { runtime.setWorkerKeepAlive(PRODUCT.value, WorkerKeepAlive.FOREGROUND) }
        verify(exactly = 0) { runtime.setWorkerKeepAlive(PRODUCT.value, WorkerKeepAlive.ON_DEMAND) }

        supervisor.withdrawForegroundWorker(runtime, PRODUCT)
        supervisor.withdrawForegroundWorker(runtime, PRODUCT)
        verify(exactly = 1) { runtime.setWorkerKeepAlive(PRODUCT.value, WorkerKeepAlive.ON_DEMAND) }
    }

    private fun TestScope.supervisor() = TrUAPIWorkerSupervisor(
        runtimeProvider = { mockk { coEvery { runtime() } returns Result.success(runtime) } },
        hostBridgeFactory = object : ProductTrUAPIHostBridge.Factory {
            override fun create(scope: CoroutineScope) = bridge()
        },
        chainDirectory = mockk { coEvery { resolve() } returns mockk() },
        scriptResolver = object : ProductScriptResolver {
            override suspend fun resolveWorker(productId: ProductId) = Result.success(
                mockk<ProductExecutable.Worker> {
                    every { scriptUrl } returns ENTRY_URL
                    every { includesChat } returns true
                },
            )
        },
        webViewProviderFactory = object : ChatWebViewProvider.Factory {
            override fun create(config: ChatWebViewConfig, scope: CoroutineScope): ChatWebViewProvider {
                webViews++
                return mockk(relaxed = true)
            }
        },
        bootstrapInstaller = mockk(relaxed = true),
        refCounter = { mockk(relaxed = true) },
        chatStoredState = mockk(),
        workerCode = mockk { coEvery { load(any(), any()) } returns Result.success(TrUAPIWorkerCode.Wasm(MODULE, "wasm.dot/index.wasm")) },
        dispatchers = testDispatchers(),
    )

    private fun bridge(): ProductTrUAPIHostBridge = mockk {
        val end = CompletableDeferred<NativeWasmWorkerEnd>().also { ends += it }
        val execution = mockk<TrUAPIProductExecution> {
            every { startWasmWorker(any()) } returns end
        }.also { executions += it }
        coEvery { attach(any(), any(), any(), any(), any(), any(), any(), any()) } answers {
            chatBridges += arg<ChatHostBridge?>(5)
            bootstraps += arg<(suspend (String) -> Unit)?>(7)
            Result.success(execution)
        }
    }

    private companion object {
        val PRODUCT = ProductId.fromStoredValue("wasm.dot")
        val MODULE = byteArrayOf(0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00)
        const val ENTRY_URL = "file:///data/user/0/app/files/Products/wasm.dot/ChatExtension/index.wasm"
    }
}
