package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContact
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RealContactDisplayProviderTest {
    private val alice = DataByteArray(ByteArray(32) { 0xA1.toByte() })
    private val bob = DataByteArray(ByteArray(32) { 0xB0.toByte() })
    private val carol = DataByteArray(ByteArray(32) { 0xC0.toByte() })
    private val nativeAlice = mockk<Contact>()
    private val contacts = mockk<ContactsRepository> {
        coEvery { getContact(any()) } returns null
        coEvery { getContact(alice) } returns nativeAlice
    }
    private val chatEngine = mockk<ChatEngine> {
        every { getContactChatDisplay(nativeAlice) } returns ChatDisplay("alice", ChatAvatar.Url("https://a"))
    }
    private val personChat = mockk<PersonChatContacts> {
        coEvery { getContacts() } returns listOf(
            PersonChatContact(alice, "alice.product", ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-a1"), blocked = false),
            PersonChatContact(bob, "bob.01", ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-b0"), blocked = false),
        )
    }
    private fun provider(productChat: Boolean) = RealContactDisplayProvider(
        contacts,
        chatEngine,
        personChat,
        ChatDisplayGenerator(mockk()),
        mockk<PersonChatMode> { every { isProductChat() } returns productChat },
    )

    @Test
    fun `a caller the chat product knows gets its name, and a native contact keeps the native one`() = runTest {
        val provider = provider(productChat = true)
        assertEquals("alice", provider.getContactDisplay(alice)?.username)
        assertEquals("bob.01", provider.getContactDisplay(bob)?.username)
        assertNull(provider.getContactDisplay(carol))
    }

    @Test
    fun `the native chat knows only its own contacts`() = runTest {
        val provider = provider(productChat = false)
        assertEquals("alice", provider.getContactDisplay(alice)?.username)
        assertNull(provider.getContactDisplay(bob))
    }
}
