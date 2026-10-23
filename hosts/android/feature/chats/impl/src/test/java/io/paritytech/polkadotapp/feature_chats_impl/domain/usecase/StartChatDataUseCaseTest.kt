package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContact
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.StartChatData
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class StartChatDataUseCaseTest {
    private val home = ChatId.forExtensionRoom("ProductBot_chat.paseo", "chats")
    private val bobRoom = ChatId.forExtensionRoom("ProductBot_chat.paseo", "dm-b0")
    private val bob = DataByteArray(ByteArray(32) { 0xB0.toByte() })
    private val querying = mockk<ProductChatQuerying> { coEvery { openLink(home, any()) } returns Result.success(bobRoom) }
    private val registry = mockk<ChatExtensionRegistry> {
        coEvery { getExtensionForChat(home) } returns mockk<ChatExtension> { every { roomQuerying(home) } returns querying }
    }
    private val useCase = StartChatDataUseCase(
        knownChains = KnownChains(people = "people", assetHub = "asset-hub", bulletIn = "bullet-in", hydration = null),
        chainRegistry = mockk(),
        resourcesRepository = mockk(),
        personChatRooms = mockk<PersonChatRooms> { every { homeRoom() } returns home },
        chatExtensionRegistry = registry,
        personChatContacts = mockk<PersonChatContacts> {
            coEvery { getContacts() } returns listOf(PersonChatContact(bob, "bob.01", bobRoom, blocked = false))
        },
    )

    // The chat product runs the person chats, so a chat started anywhere in the app opens in it (HX2).
    @Test
    fun `a chat with an account opens the product's room for it`() = runTest {
        val started = useCase(bob).getOrThrow()

        assertEquals(StartChatData.ProductRoom(bobRoom), started)
        coVerify { querying.openLink(home, "contact:" + "b0".repeat(32)) }
    }
}
