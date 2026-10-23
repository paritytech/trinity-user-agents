package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContact
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RealContactDisplayProviderTest {
    private val alice = DataByteArray(ByteArray(32) { 0xA1.toByte() })
    private val bob = DataByteArray(ByteArray(32) { 0xB0.toByte() })
    private val carol = DataByteArray(ByteArray(32) { 0xC0.toByte() })
    private val personChat = mockk<PersonChatContacts> {
        coEvery { getContacts() } returns listOf(
            PersonChatContact(alice, "alice.product", ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-a1"), blocked = false),
            PersonChatContact(bob, "bob.01", ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-b0"), blocked = false),
        )
    }
    private val provider = RealContactDisplayProvider(personChat, ChatDisplayGenerator(mockk()))

    @Test
    fun `a caller is named as the chat product knows it, and an account it does not know gets nothing`() = runTest {
        assertEquals("alice.product", provider.getContactDisplay(alice)?.username)
        assertEquals("bob.01", provider.getContactDisplay(bob)?.username)
        assertNull(provider.getContactDisplay(carol))
    }
}
