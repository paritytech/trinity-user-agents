package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionPromptUnavailableException
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.PermissionPromptBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.PermissionPromptViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class PermissionPromptCancellationTest {
    private val main = UnconfinedTestDispatcher()
    private val holder = PermissionContextHolder()
    private val router = PromptNavigation()
    private val lifecycle = AppLifecycle()
    private val requester = RealProductPermissionRequester(holder, router, lifecycle, 5.seconds)
    private val product = ProductId.fromStoredValue("product.paseo")

    @Before
    fun setUp() {
        Dispatchers.setMain(main)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `late prompt creation does not crash after its request is cancelled`() = runBlocking<Unit> {
        val pending = requestPermission()
        yield()
        val id = requireNotNull(holder.get()).id

        pending.cancel()
        yield()
        pending.join()
        val viewModel = prompt(id)
        yield()

        assertEquals(listOf(null, null, null), listOf(viewModel.state.value, holder.get(), router.currentId))
    }

    @Test
    fun `a cancelled prompt cannot approve or close the next request`() = runBlocking<Unit> {
        val first = requestPermission()
        yield()
        val oldId = requireNotNull(holder.get()).id
        val oldViewModel = prompt(oldId)
        first.cancel()
        yield()
        first.join()
        val next = requestPermission()
        yield()
        val nextId = requireNotNull(holder.get()).id

        val lateViewModel = prompt(oldId)
        lateViewModel.onAllowAlwaysClicked()
        oldViewModel.onAllowOnceClicked()
        yield()

        assertEquals(listOf(null, nextId, false), listOf(lateViewModel.state.value, router.currentId, next.isCompleted))
        prompt(nextId).onDenyClicked()
        yield()
        assertEquals(PermissionDecision.Deny, next.await())
    }

    @Test
    fun `each decision closes its prompt and reaches the requester`() = runBlocking<Unit> {
        for (decision in listOf(PermissionDecision.AllowOnce, PermissionDecision.AllowAlways, PermissionDecision.Deny)) {
            val pending = requestPermission()
            yield()
            val id = requireNotNull(holder.get()).id
            val viewModel = prompt(id)

            when (decision) {
                PermissionDecision.AllowOnce -> viewModel.onAllowOnceClicked()
                PermissionDecision.AllowAlways -> viewModel.onAllowAlwaysClicked()
                PermissionDecision.Deny -> viewModel.onDenyClicked()
            }
            yield()

            assertEquals(listOf(decision, null, null), listOf(pending.await(), holder.get(), router.currentId))
            assertEquals(listOf("open:$id", "close:$id"), router.events.takeLast(2))
        }
    }

    @Test
    fun `cancelled cleanup completes before the next prompt starts`() = verifyCleanup(cancel = true)

    @Test
    fun `answered cleanup completes before the next prompt starts`() = verifyCleanup(cancel = false)

    @Test
    fun `restored prompt without a live context closes without authority`() = runBlocking<Unit> {
        val viewModel = prompt(null)
        viewModel.onAllowAlwaysClicked()
        yield()

        assertNull(viewModel.state.value)
        assertEquals(listOf("close:null"), router.events)
    }

    @Test
    fun `resuming a cancelled prompt retries dismissal ignored while backgrounded`() = runBlocking<Unit> {
        val pending = requestPermission()
        yield()
        val id = requireNotNull(holder.get()).id
        val viewModel = prompt(id)
        router.stateSaved = true

        pending.cancel()
        yield()
        pending.join()
        assertEquals(listOf(null, id), listOf(holder.get(), router.currentId))
        router.stateSaved = false
        viewModel.onResume()
        yield()

        assertNull(router.currentId)
    }

    @Test
    fun `a prompt that leaves the screen with no answer ends its request, and the next request shows its prompt`() =
        runBlocking<Unit> {
            val first = requestPermission()
            yield()
            val firstId = requireNotNull(holder.get()).id
            val store = ViewModelStore()
            promptIn(store, firstId)

            store.clear()
            yield()

            assertTrue(runCatching { first.await() }.exceptionOrNull() is PermissionPromptUnavailableException)
            assertEquals(listOf(null, null), listOf(holder.get(), router.currentId))
            val next = requestPermission()
            yield()
            prompt(requireNotNull(holder.get()).id).onAllowOnceClicked()
            yield()
            assertEquals(PermissionDecision.AllowOnce, next.await())
        }

    @Test
    fun `an answer before the prompt leaves the screen stays the answer`() = runBlocking<Unit> {
        val pending = requestPermission()
        yield()
        val store = ViewModelStore()
        promptIn(store, requireNotNull(holder.get()).id).onDenyClicked()

        store.clear()
        yield()

        assertEquals(PermissionDecision.Deny, pending.await())
    }

    @Test
    fun `a request fails at once when no screen can show its prompt`() = runBlocking<Unit> {
        router.canShow = false

        val pending = requestPermission()
        yield()

        assertTrue(runCatching { pending.await() }.exceptionOrNull() is PermissionPromptUnavailableException)
        assertNull(holder.get())
        assertEquals(2, router.events.size)
        router.canShow = true
        val next = requestPermission()
        yield()
        assertEquals(false, next.isCompleted)
        prompt(requireNotNull(holder.get()).id).onAllowAlwaysClicked()
        yield()
        assertEquals(PermissionDecision.AllowAlways, next.await())
    }

    @Test
    fun `a request from the background shows its prompt when the app comes to the front`() = runBlocking<Unit> {
        lifecycle.state.value = AppLifecycleState.BACKGROUND

        val pending = requestPermission()
        yield()
        assertEquals(listOf(null, emptyList<String>()), listOf(holder.get(), router.events))
        lifecycle.state.value = AppLifecycleState.FOREGROUND
        yield()
        prompt(requireNotNull(holder.get()).id).onAllowOnceClicked()
        yield()

        assertEquals(PermissionDecision.AllowOnce, pending.await())
    }

    @Test
    fun `a request fails when the app stays in the background`() = runBlocking<Unit> {
        lifecycle.state.value = AppLifecycleState.BACKGROUND
        val shortWait = RealProductPermissionRequester(holder, router, lifecycle, 50.milliseconds)

        val failure = runCatching { shortWait.prompt(product, ProductPermission.BalanceAccess) }.exceptionOrNull()

        assertTrue(failure is PermissionPromptUnavailableException)
        assertEquals(listOf(null, emptyList<String>()), listOf(holder.get(), router.events))
    }

    private fun verifyCleanup(cancel: Boolean) = runBlocking<Unit> {
        val first = requestPermission()
        yield()
        val firstContext = requireNotNull(holder.get())
        val closing = CompletableDeferred<Unit>()
        router.closeGate = closing
        if (cancel) first.cancel() else firstContext.deliver(PermissionDecision.AllowOnce)
        yield()
        val next = requestPermission()
        yield()

        try {
            assertEquals(listOf(firstContext, false, false), listOf(holder.get(), first.isCompleted, next.isCompleted))
            assertEquals(listOf("open:${firstContext.id}"), router.events)
        } finally {
            closing.complete(Unit)
        }
        yield()
        first.join()
        val nextContext = requireNotNull(holder.get())
        assertFalse(firstContext.id == nextContext.id)
        assertEquals(listOf("open:${firstContext.id}", "close:${firstContext.id}", "open:${nextContext.id}"), router.events)
        nextContext.deliver(PermissionDecision.Deny)
        yield()
        assertEquals(PermissionDecision.Deny, next.await())
    }

    private fun promptIn(store: ViewModelStore, id: String): PermissionPromptViewModel {
        val factory = object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <T : ViewModel> create(modelClass: Class<T>): T = prompt(id) as T
        }
        return ViewModelProvider(store, factory)[PermissionPromptViewModel::class.java]
    }

    private fun prompt(id: String?) = PermissionPromptViewModel(
        SavedStateHandle(mapOf(PermissionPromptBottomSheet.REQUEST_ID to id)), holder, router,
    ).also { it.onResume() }

    // A separate parent: a request that fails must not cancel the test. Each test waits for its requests.
    private fun requestPermission() = CoroutineScope(main + SupervisorJob()).async(start = CoroutineStart.UNDISPATCHED) {
        requester.prompt(product, ProductPermission.BalanceAccess)
    }

    private class PromptNavigation : ProductsRouter by mock(ProductsRouter::class.java) {
        var currentId: String? = null
        var closeGate: CompletableDeferred<Unit>? = null
        var stateSaved = false
        var canShow = true
        val events = mutableListOf<String>()

        override fun back() = error("Permission prompts must close by request ID")

        override suspend fun openPermissionPrompt(requestId: String) = withContext(Dispatchers.Main) {
            events += "open:$requestId"
            if (canShow) currentId = requestId
            canShow
        }

        override suspend fun closePermissionPrompt(requestId: String?) = withContext(Dispatchers.Main) {
            closeGate?.await()
            events += "close:$requestId"
            if (!stateSaved && currentId == requestId) currentId = null
        }
    }
}

private class AppLifecycle : AppLifecycleObserver {
    val state = MutableStateFlow(AppLifecycleState.FOREGROUND)

    override fun subscribe(): Flow<AppLifecycleState> = state

    override fun getCurrentState(): AppLifecycleState = state.value
}
