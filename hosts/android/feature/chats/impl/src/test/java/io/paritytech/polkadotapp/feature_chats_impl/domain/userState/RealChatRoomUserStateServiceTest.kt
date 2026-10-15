@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_chats_impl.domain.userState

import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMuteDuration
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomUserStateWriter
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatRoomEventPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatRoomUserStates
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealChatRoomUserStateServiceTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")
    private val person = ChatId.fromRawValue(byteArrayOf(1, 2, 3))
    private val now = 1_000_000L

    private val stored = MutableStateFlow(ChatRoomUserStateModel.NONE)
    private val repository = object : ChatRoomUserStateRepository {
        override fun observe(chatId: ChatId): Flow<ChatRoomUserStateModel> = stored

        override fun observeAll(): Flow<Map<ChatId, ChatRoomUserStateModel>> = stored.map { mapOf(room to it) }
    }
    private val writer = object : ChatRoomUserStateWriter {
        override suspend fun set(chatId: ChatId, state: ChatRoomUserStateModel) {
            stored.value = state
        }
    }
    private val publisher: ChatRoomEventPublisher = mockk(relaxed = true)
    private val time = object : TimeProvider {
        override fun now(): Instant = Instant.fromEpochMilliseconds(now)
    }

    private val service = RealChatRoomUserStateService(repository, ChatRoomUserStates(writer, publisher), time)

    @Test
    fun `a mute for one hour ends one hour from now and the product hears of it`() = runBlocking<Unit> {
        service.mute(room, ChatMuteDuration.ONE_HOUR).getOrThrow()

        val expected = ChatRoomUserStateModel(mutedUntil = now + 3_600_000L, archived = false)
        assertEquals(expected, stored.value)
        coVerify { publisher.publish(room, ChatRoomEventModel.UserStateChanged(expected)) }
    }

    @Test
    fun `each duration ends where 07-LATER puts it`() {
        assertEquals(now + 8 * 3_600_000L, ChatMuteDuration.EIGHT_HOURS.mutedUntil(now))
        assertEquals(now + 7 * 24 * 3_600_000L, ChatMuteDuration.ONE_WEEK.mutedUntil(now))
        assertEquals(ChatRoomUserStateModel.MUTED_WITHOUT_END, ChatMuteDuration.ALWAYS.mutedUntil(now))
    }

    @Test
    fun `archive keeps the mute, unarchive and unmute clear their part`() = runBlocking<Unit> {
        service.mute(room, ChatMuteDuration.ALWAYS).getOrThrow()
        service.setArchived(room, archived = true).getOrThrow()
        assertEquals(ChatRoomUserStateModel(ChatRoomUserStateModel.MUTED_WITHOUT_END, archived = true), stored.value)

        service.unmute(room).getOrThrow()
        assertEquals(ChatRoomUserStateModel(mutedUntil = null, archived = true), stored.value)

        service.setArchived(room, archived = false).getOrThrow()
        assertEquals(ChatRoomUserStateModel.NONE, stored.value)
    }

    @Test
    fun `an action that changes nothing does not tell the product`() = runBlocking<Unit> {
        service.unmute(room).getOrThrow()
        service.setArchived(room, archived = false).getOrThrow()

        coVerify(exactly = 0) { publisher.publish(any(), any()) }
    }

    @Test
    fun `a mute that ended goes with the next change, so the product gets no old end time`() = runBlocking<Unit> {
        stored.value = ChatRoomUserStateModel(mutedUntil = now - 1, archived = false)

        service.setArchived(room, archived = true).getOrThrow()

        val expected = ChatRoomUserStateModel(mutedUntil = null, archived = true)
        assertEquals(expected, stored.value)
        coVerify { publisher.publish(room, ChatRoomEventModel.UserStateChanged(expected)) }
    }

    @Test
    fun `a person chat has no mute and no archive`() = runBlocking<Unit> {
        val outcome = service.mute(person, ChatMuteDuration.ONE_HOUR)

        assertTrue(outcome.isFailure)
        coVerify(exactly = 0) { publisher.publish(any(), any()) }
    }
}
