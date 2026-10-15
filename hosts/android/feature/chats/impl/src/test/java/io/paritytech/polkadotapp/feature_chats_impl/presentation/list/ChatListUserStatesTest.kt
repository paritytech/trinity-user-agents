package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ChatListUserStatesTest {
    @Test
    fun `archived rooms leave the main section in the order of the list`() {
        val first = room("a", archived = true)
        val second = room("b")
        val third = room("c", archived = true)

        val sections = listOf(first, second, third, person("p")).toSections()

        assertEquals(listOf("b", "p"), sections.main.map { it.display.name })
        assertEquals(listOf("a", "c"), sections.archived.map { it.display.name })
    }

    @Test
    fun `a mute counts until its end, and a person chat has no user state`() {
        val muted = room("a", mutedUntil = 2_000)

        assertEquals(ChatListUiState.UserState(isMuted = true, isArchived = false), muted.userStateAt(1_999))
        assertEquals(ChatListUiState.UserState(isMuted = false, isArchived = false), muted.userStateAt(2_000))
        assertEquals(ChatListUiState.UserState(isMuted = true, isArchived = false), room("b", ChatRoomUserStateModel.MUTED_WITHOUT_END).userStateAt(Long.MAX_VALUE - 1))
        assertNull(person("p").userStateAt(0))
    }

    @Test
    fun `the next end of a mute skips ended mutes and mutes without end`() {
        val chats = listOf(
            room("a", mutedUntil = 500),
            room("b", mutedUntil = 3_000),
            room("c", mutedUntil = 2_000),
            room("d", mutedUntil = ChatRoomUserStateModel.MUTED_WITHOUT_END),
        )

        assertEquals(2_000L, nextMuteEnd(chats, nowMs = 1_000))
        assertNull(nextMuteEnd(listOf(room("d", ChatRoomUserStateModel.MUTED_WITHOUT_END)), nowMs = 1_000))
    }

    @Test
    fun `the list comes again when a mute ends`() = runTest {
        val start = testScheduler.currentTime
        val chats = listOf(room("a", mutedUntil = start + 60_000))

        val emitted = flowOf(chats).withMuteClock { testScheduler.currentTime }.take(2).toList()

        assertEquals(listOf(start, start + 60_000), emitted.map { it.second })
        assertEquals(false, emitted.last().first.single().userStateAt(emitted.last().second)?.isMuted)
    }

    private fun room(name: String, mutedUntil: Long? = null, archived: Boolean = false) = chat(
        id = ChatId.forExtensionRoom("ProductBot_sample.dot", name),
        name = name,
        policy = ChatRoomPolicy.Defaults.copy(userState = ChatRoomUserStateModel(mutedUntil, archived)),
    )

    private fun person(name: String) = chat(ChatId.fromRawValue(name.encodeToByteArray()), name, policy = null)

    private fun chat(id: ChatId, name: String, policy: ChatRoomPolicy?) = Chat(
        id = id,
        display = ChatDisplay(name, ChatAvatar.Account(name, byteArrayOf(1))),
        preview = ChatPreview.EmptyChat,
        timestamp = 0,
        unreadBadge = ChatSummaryBadge.None,
        hasUnseenReaction = false,
        customPreviewRenderer = null,
        roomPolicy = policy,
    )
}
