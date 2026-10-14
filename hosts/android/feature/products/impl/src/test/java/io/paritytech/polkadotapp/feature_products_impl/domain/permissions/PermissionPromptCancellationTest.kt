package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import androidx.lifecycle.SavedStateHandle
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.PermissionPromptBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.PermissionPromptViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
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
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock

@OptIn(ExperimentalCoroutinesApi::class)
class PermissionPromptCancellationTest {
    private val main = UnconfinedTestDispatcher()
    private val holder = PermissionContextHolder()
    private val router = PromptNavigation()
    private val requester = RealProductPermissionRequester(holder, router)
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

    private fun prompt(id: String?) = PermissionPromptViewModel(
        SavedStateHandle(mapOf(PermissionPromptBottomSheet.REQUEST_ID to id)), holder, router,
    ).also { it.onResume() }

    private fun CoroutineScope.requestPermission() = async(main, start = CoroutineStart.UNDISPATCHED) {
        requester.prompt(product, ProductPermission.BalanceAccess)
    }

    private class PromptNavigation : ProductsRouter by mock(ProductsRouter::class.java) {
        var currentId: String? = null
        var closeGate: CompletableDeferred<Unit>? = null
        var stateSaved = false
        val events = mutableListOf<String>()

        override fun back() = error("Permission prompts must close by request ID")

        override suspend fun openPermissionPrompt(requestId: String) = withContext(Dispatchers.Main) {
            currentId = requestId
            events += "open:$requestId"
        }

        override suspend fun closePermissionPrompt(requestId: String?) = withContext(Dispatchers.Main) {
            closeGate?.await()
            events += "close:$requestId"
            if (!stateSaved && currentId == requestId) currentId = null
        }
    }
}
