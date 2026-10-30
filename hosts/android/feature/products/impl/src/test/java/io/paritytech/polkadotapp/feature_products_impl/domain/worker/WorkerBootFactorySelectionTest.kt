package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import dagger.Lazy
import io.mockk.Called
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.HostApiProductsScriptExecutor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIChatWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class WorkerBootFactorySelectionTest {
    private val productId = ProductId.fromStoredValue("chat.dot")
    private val botApi = BindableProductsBotApi(mockk(), mockk())

    private fun TestScope.factory(
        runtimeSettings: ProductRuntimeSettings,
        runtimeProvider: TrUAPIHostRuntimeProvider = mockk(),
        workerSupervisor: TrUAPIWorkerSupervisor = mockk(),
        scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mockk(),
    ): RealWorkerBootFactory = RealWorkerBootFactory(
        scriptExecutorFactory = scriptExecutorFactory,
        runtimeSettings = runtimeSettings,
        runtimeProvider = Lazy { runtimeProvider },
        workerSupervisor = Lazy { workerSupervisor },
    )

    private fun settingsWith(enabled: Boolean): ProductRuntimeSettings {
        val settings: ProductRuntimeSettings = mockk()
        every { settings.isTrUAPIRuntimeEnabled() } returns enabled
        return settings
    }

    private suspend fun jsExecutorFactory(executor: HostApiProductsScriptExecutor, scope: CoroutineScope): HostApiProductsScriptExecutor.Factory {
        val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mockk()
        every { scriptExecutorFactory.create(productId) } returns executor
        coEvery { executor.initializeBot(botApi, scope) } returns Result.success(Unit)
        return scriptExecutorFactory
    }

    @Test
    fun `a JS boot failure surfaces instead of becoming a silent null`() = runTest {
        val executor: HostApiProductsScriptExecutor = mockk()
        val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mockk()
        every { scriptExecutorFactory.create(productId) } returns executor
        val boom = IllegalStateException("no worker script")
        coEvery { executor.initializeBot(botApi, this@runTest) } returns Result.failure(boom)
        val factory = factory(settingsWith(enabled = false), scriptExecutorFactory = scriptExecutorFactory)

        val failure = runCatching { factory.boot(productId, botApi, this) }.exceptionOrNull()

        assertSame("the JS arm must carry its cause, like the core arm", boom, failure)
    }

    @Test
    fun `setting off boots the JS worker`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mockk()
        val executor: HostApiProductsScriptExecutor = mockk()
        val scriptExecutorFactory = jsExecutorFactory(executor, this)
        val factory = factory(
            settingsWith(enabled = false),
            runtimeProvider = runtimeProvider,
            scriptExecutorFactory = scriptExecutorFactory,
        )

        val worker = factory.boot(productId, botApi, this)

        assertSame("setting off must boot the JS worker unchanged", executor, worker)
        verify { runtimeProvider wasNot Called }
    }

    @Test
    fun `setting on boots the core worker`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mockk()
        val runtime: TrUAPIHostRuntime = mockk(relaxUnitFun = true)
        coEvery { runtimeProvider.runtime() } returns Result.success(runtime)
        val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mockk()
        val workerSupervisor: TrUAPIWorkerSupervisor = mockk()
        every { workerSupervisor.executionState(productId) } returns emptyFlow()
        val factory = factory(
            settingsWith(enabled = true),
            runtimeProvider = runtimeProvider,
            workerSupervisor = workerSupervisor,
            scriptExecutorFactory = scriptExecutorFactory,
        )
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler))

        val worker = factory.boot(productId, botApi, scope)
        advanceUntilIdle()

        assertTrue("setting on must boot the core worker", worker is TrUAPIChatWorker)
        verify { scriptExecutorFactory wasNot Called }
    }

    @Test
    fun `a construction failure on the core arm surfaces instead of falling back`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mockk()
        val runtime: TrUAPIHostRuntime = mockk()
        coEvery { runtimeProvider.runtime() } returns Result.success(runtime)
        every { runtime.acquireWorker(productId.value) } throws IllegalStateException("acquireWorker boom")
        val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mockk()
        val factory = factory(
            settingsWith(enabled = true),
            runtimeProvider = runtimeProvider,
            scriptExecutorFactory = scriptExecutorFactory,
        )

        val failure = runCatching { factory.boot(productId, botApi, this) }.exceptionOrNull()

        assertTrue("a core boot failure must surface", failure is IllegalStateException)
        verify { scriptExecutorFactory wasNot Called }
    }
}
