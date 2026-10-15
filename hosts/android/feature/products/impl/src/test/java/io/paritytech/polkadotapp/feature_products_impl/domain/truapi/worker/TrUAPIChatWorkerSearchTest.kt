package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.justRun
import io.mockk.mockk
import io.parity.truapi.ChatQueryException
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductChatQueryError
import uniffi.truapi.ProductChatSearchQueryItem
import uniffi.truapi.ProductChatSearchQueryRequest

/** The in-room search of a room whose product keeps the messages goes to the product as `search_query` (07-LATER §8). */
class TrUAPIChatWorkerSearchTest {
    private val productId = ProductId.fromStoredValue("game.dot")
    private val room = ProductChatIdParameter("lobby")
    private val execution: TrUAPIProductExecution = mockk(relaxed = true)
    private val runtime: TrUAPIHostRuntime = mockk(relaxed = true)

    @Test
    fun `the search asks search_query with the text, the cursor and the page size`() = runTest {
        coEvery { execution.chatSearchQuery(any(), any()) } returns ProductChatSearchQueryItem(emptyList(), next = "c2")

        val page = worker().searchPage(room, "crème", cursor = "c1", pageSize = 30)

        assertEquals("c2", page.getOrThrow().next)
        coVerify { execution.chatSearchQuery(ProductChatSearchQueryRequest("lobby", "crème", "c1", 30u), any()) }
    }

    @Test
    fun `a product that does not search answers NotAvailable`() = runTest {
        coEvery { execution.chatSearchQuery(any(), any()) } throws ChatQueryException.Refused(ProductChatQueryError.NotAvailable)

        val page = worker().searchPage(room, "x", cursor = null, pageSize = 30)

        assertTrue(page.exceptionOrNull() is ProductChatQueryException.NotAvailable)
    }

    private fun TestScope.worker(): TrUAPIChatWorker {
        val workers: TrUAPIWorkerSupervisor = mockk()
        justRun { workers.restartIfFailed(productId) }
        every { workers.executionState(productId) } returns flowOf(WorkerExecutionState.Running(execution))
        every { workers.currentExecution(productId) } returns execution
        return TrUAPIChatWorker(productId, runtime, workers, FakeChatMessaging(), CoroutineScope(StandardTestDispatcher(testScheduler)))
    }
}
