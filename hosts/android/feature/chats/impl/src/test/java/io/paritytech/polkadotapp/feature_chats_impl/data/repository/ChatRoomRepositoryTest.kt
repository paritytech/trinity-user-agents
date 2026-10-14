package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.database.dao.ChatRoomDao
import io.paritytech.polkadotapp.database.model.ChatRoomLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatRoomRepositoryTest {
    private val chatId = ChatId.forExtensionRoom("dim2.paseo", "jollity")
    private val dao: ChatRoomDao = mockk(relaxed = true)
    private val repository = RealChatRoomRepository(dao)

    private fun storedRoom(name: String?, icon: String?) = ChatRoomLocal(chatId.toLocal(), createdAt = 1L, name = name, icon = icon)

    // A product registers its room on every start and its API has no call that updates one, so
    // this is how a room placed by the host before the product ran gets the product's own icon.
    @Test
    fun `a room registered again under other metadata takes it`() = runTest {
        coEvery { dao.getRoom(chatId.toLocal()) } returns storedRoom(name = "Jollity", icon = null)

        val result = repository.createRoom(CreateRoomRequest(chatId, name = "Jollity", icon = "ipfs://icon"))

        assertEquals(CreateRoomStatus.Exists, result.status)
        coVerify { dao.updateMetadata(chatId.toLocal(), "Jollity", "ipfs://icon") }
    }

    // The host places a room with no metadata on every start; the name and icon the product
    // supplied on a previous run must survive that, or every launch shows the placeholder again.
    @Test
    fun `a registration without metadata leaves a named room alone`() = runTest {
        coEvery { dao.getRoom(chatId.toLocal()) } returns storedRoom(name = "Jollity", icon = "ipfs://icon")

        val result = repository.createRoom(CreateRoomRequest(chatId, name = null, icon = null))

        assertEquals(CreateRoomStatus.Exists, result.status)
        coVerify(exactly = 0) { dao.updateMetadata(any(), any(), any()) }
    }

    @Test
    fun `a room registered again unchanged is left alone`() = runTest {
        coEvery { dao.getRoom(chatId.toLocal()) } returns storedRoom(name = "Jollity", icon = "ipfs://icon")

        val result = repository.createRoom(CreateRoomRequest(chatId, name = "Jollity", icon = "ipfs://icon"))

        assertEquals(CreateRoomStatus.Exists, result.status)
        coVerify(exactly = 0) { dao.updateMetadata(any(), any(), any()) }
    }
}
