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

private const val PNG_ICON = "data:image/png;base64,iVBORw=="

class RealChatRoomRepositoryTest {
    private val chatRoomDao: ChatRoomDao = mockk(relaxUnitFun = true)
    private val repository = RealChatRoomRepository(chatRoomDao)
    private val chatId = ChatId.forExtensionRoom("dim2.paseo", "jollity")

    // The host places Jollity before its worker runs, so the worker's own createRoom always finds the
    // room. That call is the only time the product says what its room looks like.
    @Test
    fun `an existing room takes the name and icon the product registers`() = runTest {
        withRoom(name = null, icon = null)

        val status = repository.createRoom(CreateRoomRequest(chatId, "Jollity", PNG_ICON)).status

        assertEquals(CreateRoomStatus.Exists, status)
        coVerify { chatRoomDao.updateMetadata(chatId.toLocal(), "Jollity", PNG_ICON) }
    }

    @Test
    fun `a room registered again unchanged on the next start is not rewritten`() = runTest {
        withRoom(name = "Jollity", icon = PNG_ICON)

        repository.createRoom(CreateRoomRequest(chatId, "Jollity", PNG_ICON))

        coVerify(exactly = 0) { chatRoomDao.updateMetadata(any(), any(), any()) }
    }

    private fun withRoom(name: String?, icon: String?) {
        coEvery { chatRoomDao.getRoom(chatId.toLocal()) } returns ChatRoomLocal(chatId.toLocal(), createdAt = 0, name, icon)
    }
}
