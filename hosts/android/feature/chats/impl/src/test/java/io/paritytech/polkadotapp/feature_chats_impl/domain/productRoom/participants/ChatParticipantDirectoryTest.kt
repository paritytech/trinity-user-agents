package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.participants

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ChatParticipantDirectoryTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "lobby")
    private val querying = mockk<ProductChatQuerying> {
        coEvery { participantsCount(any()) } returns Result.success(250)
        coEvery { participantsByIds(any(), any()) } answers { Result.success(secondArg<List<String>>().map(::custom)) }
        coEvery { participantsPage(any(), any(), any()) } returns Result.success(ProductParticipantsPage(listOf(custom("p1")), next = "2"))
    }

    @Test
    fun `the count is asked once`() = runTest {
        val directory = directory()

        directory.loadCount()
        directory.loadCount()
        advanceUntilIdle()

        assertEquals(250, directory.count.value)
        coVerify(exactly = 1) { querying.participantsCount(any()) }
    }

    @Test
    fun `ids are asked in batches of 100, each once, and the reserved ids never`() = runTest {
        val directory = directory()
        val ids = (1..150).map { "p$it" }

        directory.resolve(ids + listOf("user", "product"))
        directory.resolve(ids)
        advanceUntilIdle()

        assertEquals(150, directory.resolved.value.size)
        coVerify(exactly = 2) { querying.participantsByIds(any(), any()) }
        coVerify { querying.participantsByIds(any(), match { it.size == 100 }) }
        coVerify { querying.participantsByIds(any(), match { it.size == 50 }) }
    }

    @Test
    fun `a page asks 100 participants and fills the directory`() = runTest {
        val directory = directory()

        val page = directory.page(cursor = null).getOrThrow()

        assertEquals("2", page.next)
        assertEquals(custom("p1"), directory.resolved.value["p1"])
        coVerify { querying.participantsPage(any(), null, 100) }
    }

    @Test
    fun `a refusal leaves the count unknown and reset forgets everything`() = runTest {
        coEvery { querying.participantsCount(any()) } returns Result.failure(ProductChatQueryException.NotAvailable())
        val directory = directory()

        directory.loadCount()
        directory.resolve(listOf("p1"))
        advanceUntilIdle()
        assertNull(directory.count.value)

        directory.reset()
        directory.resolve(listOf("p1"))
        advanceUntilIdle()

        coVerify(exactly = 2) { querying.participantsByIds(any(), listOf("p1")) }
    }

    private fun TestScope.directory() = ChatParticipantDirectory(roomId, this) { querying }

    private fun custom(id: String) = ChatParticipantEntryModel(id, ChatParticipantModel.Custom(nickname = id, avatar = null))
}
