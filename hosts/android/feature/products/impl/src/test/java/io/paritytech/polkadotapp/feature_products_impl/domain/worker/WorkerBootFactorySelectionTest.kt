package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import dagger.Lazy
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.HostApiProductsScriptExecutor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIChatWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.test_shared.whenever
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
import org.mockito.Mockito.mock
import org.mockito.Mockito.verifyNoInteractions

@OptIn(ExperimentalCoroutinesApi::class)
class WorkerBootFactorySelectionTest {
    private val productId = ProductId.fromStoredValue("chat.dot")
    private val botApi = BindableProductsBotApi(mock(), mock())

    private fun TestScope.factory(
        runtimeSettings: ProductRuntimeSettings,
        runtimeProvider: TrUAPIHostRuntimeProvider = mock(),
        workerSupervisor: TrUAPIWorkerSupervisor = mock(),
        scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mock(),
        inAppRuntimes: Set<InAppProductRuntime> = emptySet(),
    ): RealWorkerBootFactory = RealWorkerBootFactory(
        scriptExecutorFactory = scriptExecutorFactory,
        runtimeSettings = runtimeSettings,
        runtimeProvider = Lazy { runtimeProvider },
        workerSupervisor = Lazy { workerSupervisor },
        scriptResolver = object : ProductScriptResolver {
            override suspend fun resolveWorker(productId: ProductId) = Result.failure<ProductExecutable.Worker>(IllegalStateException("no worker"))
        },
        inAppRuntimes = inAppRuntimes,
    )

    private fun settingsWith(enabled: Boolean): ProductRuntimeSettings {
        val settings: ProductRuntimeSettings = mock()
        whenever(settings.isTrUAPIRuntimeEnabled()).thenReturn(enabled)
        return settings
    }

    private suspend fun jsExecutorFactory(executor: HostApiProductsScriptExecutor, scope: CoroutineScope): HostApiProductsScriptExecutor.Factory {
        val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mock()
        whenever(scriptExecutorFactory.create(productId)).thenReturn(executor)
        whenever(executor.initializeBot(botApi, scope)).thenReturn(Result.success(Unit))
        return scriptExecutorFactory
    }

    @Test
    fun `a JS boot failure surfaces instead of becoming a silent null`() = runTest {
        val executor: HostApiProductsScriptExecutor = mock()
        val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mock()
        whenever(scriptExecutorFactory.create(productId)).thenReturn(executor)
        val boom = IllegalStateException("no worker script")
        whenever(executor.initializeBot(botApi, this)).thenReturn(Result.failure(boom))
        val factory = factory(settingsWith(enabled = false), scriptExecutorFactory = scriptExecutorFactory)

        val failure = runCatching { factory.boot(productId, botApi, this) }.exceptionOrNull()

        assertSame("the JS arm must carry its cause, like the core arm", boom, failure)
    }

    @Test
    fun `setting off boots the JS worker`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mock()
        val executor: HostApiProductsScriptExecutor = mock()
        val scriptExecutorFactory = jsExecutorFactory(executor, this)
        val factory = factory(
            settingsWith(enabled = false),
            runtimeProvider = runtimeProvider,
            scriptExecutorFactory = scriptExecutorFactory,
        )

        val worker = factory.boot(productId, botApi, this)

        assertSame("setting off must boot the JS worker unchanged", executor, worker)
        verifyNoInteractions(runtimeProvider)
    }

    @Test
    fun `setting on boots the core worker`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mock()
        val runtime: TrUAPIHostRuntime = mock()
        whenever(runtimeProvider.runtime()).thenReturn(Result.success(runtime))
        val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mock()
        val workerSupervisor: TrUAPIWorkerSupervisor = mock()
        whenever(workerSupervisor.executionState(productId)).thenReturn(emptyFlow())
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
        verifyNoInteractions(scriptExecutorFactory)
    }

    @Test
    fun `a construction failure on the core arm surfaces instead of falling back`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mock()
        whenever(runtimeProvider.runtime()).thenReturn(Result.failure(IllegalStateException("no core runtime")))
        val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mock()
        val factory = factory(
            settingsWith(enabled = true),
            runtimeProvider = runtimeProvider,
            scriptExecutorFactory = scriptExecutorFactory,
        )

        val failure = runCatching { factory.boot(productId, botApi, this) }.exceptionOrNull()

        assertTrue("a core boot failure must surface", failure is IllegalStateException)
        verifyNoInteractions(scriptExecutorFactory)
    }

    // The chat showcase of debug builds is served by the app: it must not reach the core or a script whatever the
    // runtime setting says, and a product it does not name must not reach it.
    @Test
    fun `a product that the app serves itself boots its own runtime`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mock()
        val scriptExecutorFactory: HostApiProductsScriptExecutor.Factory = mock()
        val inAppWorker: ProductWorker = mock()
        val inAppRuntime = object : InAppProductRuntime {
            override val productId = this@WorkerBootFactorySelectionTest.productId

            override fun boot(botApi: BindableProductsBotApi, scope: CoroutineScope) = inAppWorker

            override suspend fun publishRoomEvent(room: ProductChatIdParameter, event: ChatRoomEventModel) = Result.success(Unit)
        }
        val factory = factory(
            settingsWith(enabled = true),
            runtimeProvider = runtimeProvider,
            scriptExecutorFactory = scriptExecutorFactory,
            inAppRuntimes = setOf(inAppRuntime),
        )

        val worker = factory.boot(productId, botApi, this)

        assertSame(inAppWorker, worker)
        verifyNoInteractions(runtimeProvider, scriptExecutorFactory)
    }
}
