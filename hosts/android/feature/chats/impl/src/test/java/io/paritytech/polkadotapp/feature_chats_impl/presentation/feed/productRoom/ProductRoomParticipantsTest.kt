package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageOriginDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Test

class ProductRoomParticipantsTest {
    private val room = ChatId.forExtensionRoom("ProductBot_bench.dot", "lobby")
    private val entries = (0 until 300).map { ChatParticipantEntryModel("p$it", ChatParticipantModel.Custom(nickname = "Guest $it", avatar = null)) }
    private val querying = mockk<ProductChatQuerying> {
        coEvery { participantsCount(any()) } returns Result.success(entries.size)
        coEvery { participantsPage(any(), any(), any()) } answers {
            val start = secondArg<String?>()?.toInt() ?: 0
            val end = minOf(start + thirdArg<Int>(), entries.size)
            Result.success(ProductParticipantsPage(entries.subList(start, end), next = end.takeIf { it < entries.size }?.toString()))
        }
    }
    private val origins = object : MessageOriginDisplayResolver {
        override fun displayOf(messageOrigin: ChatMessageOrigin) = MessageOriginDisplay("Bench", ChatAvatar.Url("https://bench"))
    }
    private val appearances = mockk<ChatParticipantAppearanceFactory> {
        coEvery { appearance(any(), any(), any(), any()) } answers {
            val id = firstArg<String>()
            ChatParticipantUiModel(id, id, "", AvatarUiModel.Name(id, AvatarColorScheme.from(id.encodeToByteArray())))
        }
    }

    @Test
    fun `B1 each page draws only the participants that it adds`() = runBlocking<Unit> {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val participants = ProductRoomParticipants(
            scope = scope,
            chatId = room,
            roomPolicy = flowOf(ChatRoomPolicy.Defaults.copy(productQueries = setOf(ChatRoomPolicy.ProductQuery.PARTICIPANTS))),
            feedSnapshot = flowOf(
                ChatFeedSnapshot(emptyList(), emptyMap(), emptyMap(), emptyMap(), ChatPagingState.Complete, ChatRoomReadiness.READY),
            ),
            origins = flowOf(origins),
            appearances = appearances,
            querying = { querying },
        )
        withTimeout(5_000) { participants.uiState.first { it.declared } }

        for (page in 1..3) {
            participants.loadMore()
            withTimeout(5_000) { participants.uiState.first { it.list.participants.size == page * 100 && !it.list.loading } }
        }
        scope.cancel()

        assertEquals(entries.map { it.participantId }, participants.uiState.value.list.participants.map { it.participantId })
        entries.forEach { entry -> coVerify(exactly = 1) { appearances.appearance(entry.participantId, any(), any(), any()) } }
    }
}
