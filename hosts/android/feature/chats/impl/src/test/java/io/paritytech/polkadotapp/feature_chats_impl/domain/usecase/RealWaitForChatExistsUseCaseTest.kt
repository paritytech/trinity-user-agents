package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Test
import kotlin.coroutines.EmptyCoroutineContext

// A link into a room waits until the room exists. A thread room is not in the chat list (07-LATER §9.2).
class RealWaitForChatExistsUseCaseTest {
    private val chatEngine = mockk<ChatEngine>()
    private val scope = object : ComputationalScope, CoroutineScope by CoroutineScope(EmptyCoroutineContext) {}

    @Test
    fun `a room of a product that the chat list does not show still opens`() = runBlocking {
        val thread = ChatId.forExtensionRoom("ProductBot_sample.dot", "thread-host-m1")
        every { chatEngine.observeRoomExists(thread) } returns flowOf(false, true)

        withTimeout(1_000) { with(scope) { RealWaitForChatExistsUseCase(chatEngine)(thread) } }

        verify(exactly = 0) { with(scope) { chatEngine.subscribeChatSummaries() } }
    }
}
