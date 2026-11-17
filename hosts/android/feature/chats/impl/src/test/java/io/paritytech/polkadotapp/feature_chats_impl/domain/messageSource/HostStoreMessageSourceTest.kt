package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class HostStoreMessageSourceTest {
    private val chatEngine = mockk<ChatEngine>()
    private val source = HostStoreMessageSource(productRoom, chatEngine)

    // The room screen reads the host database through the source: the messages, reactions and revisions that the
    // engine gives, in one window that holds the whole room.
    @Test
    fun `the window is the whole room as the host database keeps it`() = runBlocking<Unit> {
        val message = ChatMessage(
            id = "m1",
            chatId = productRoom,
            timestamp = 1,
            origin = ChatMessageOrigin.User,
            content = ChatMessage.Content.Text("hi"),
            status = ChatMessage.Status.IS_READ,
        )
        val reactions = mapOf("m1" to listOf(ChatMessageReaction("m1", ChatMessageReactionContent("👍"), ChatMessageOrigin.User, 2)))
        val revisions = mapOf("m1" to MessageRevision("m1", ChatMessage.Content.Text("hello"), productRoom, 3))
        every { chatEngine.subscribeMessagesFeed(productRoom) } returns flowOf(listOf(message))
        every { chatEngine.subscribeReactions(productRoom) } returns flowOf(reactions)
        every { chatEngine.subscribeRevisions(productRoom) } returns flowOf(revisions)

        val snapshot = source.observeSnapshots().first()

        assertEquals(listOf(message), snapshot.messages)
        assertEquals(reactions, snapshot.reactions)
        assertEquals(revisions, snapshot.revisions)
        assertEquals(ChatPagingState.Complete, snapshot.paging)
        assertEquals(ChatRoomReadiness.READY, snapshot.readiness)
    }

    @Test
    fun `the whole room has no more pages to ask`() = runBlocking<Unit> {
        assertEquals(ChatPageOutcome.End, source.loadOlder())
        assertEquals(ChatPageOutcome.End, source.loadNewer())
        assertFalse(source.jump("m1"))
    }
}
