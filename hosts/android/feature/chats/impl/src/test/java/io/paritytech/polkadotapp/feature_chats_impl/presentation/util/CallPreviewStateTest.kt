package io.paritytech.polkadotapp.feature_chats_impl.presentation.util

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel.Call.State
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds

class CallPreviewStateTest {
    private val room = ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-01")

    @Test
    fun `a call record without an answer and a close rings`() {
        assertEquals(State.Ringing, resolveContactCallState(offer(outgoing = true), call(answeredAt = null, closedAt = null)))
    }

    @Test
    fun `an answered call that is not closed is ongoing`() {
        assertEquals(State.Ongoing, resolveContactCallState(offer(outgoing = false), call(answeredAt = 1_500, closedAt = null)))
    }

    @Test
    fun `an answered and closed call lasted from the answer to the close`() {
        val state = resolveContactCallState(offer(outgoing = true), call(answeredAt = 1_500, closedAt = 4_500))

        assertEquals(State.Ended(3_000.milliseconds), state)
    }

    @Test
    fun `an unanswered call reads as the native person chat reads it`() {
        assertEquals(
            State.Canceled(2_000.milliseconds),
            resolveContactCallState(offer(outgoing = true), call(answeredAt = null, closedAt = 3_000, closedByUser = true)),
        )
        assertEquals(
            State.Declined(2_000.milliseconds),
            resolveContactCallState(offer(outgoing = true), call(answeredAt = null, closedAt = 3_000, closedByUser = false)),
        )
        assertEquals(
            State.Missed,
            resolveContactCallState(offer(outgoing = false), call(answeredAt = null, closedAt = 3_000, closedByUser = false)),
        )
        assertEquals(
            State.Declined(2_000.milliseconds),
            resolveContactCallState(offer(outgoing = false), call(answeredAt = null, closedAt = 3_000, closedByUser = true)),
        )
    }

    private fun call(answeredAt: Long?, closedAt: Long?, closedByUser: Boolean = false) =
        ChatMessage.Content.ContactCall(video = false, answeredAt = answeredAt, closedAt = closedAt, closedByUser = closedByUser)

    private fun offer(outgoing: Boolean) = ChatMessage(
        id = "call-1",
        chatId = room,
        timestamp = 1_000,
        origin = if (outgoing) ChatMessageOrigin.User else ChatMessageOrigin.Extension("ProductBot_chat.dot"),
        content = call(answeredAt = null, closedAt = null),
        status = ChatMessage.Status.IS_READ,
    )
}
