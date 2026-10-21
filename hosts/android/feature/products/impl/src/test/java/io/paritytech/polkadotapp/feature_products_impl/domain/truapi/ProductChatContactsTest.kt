package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.HostChatContact

class ProductChatContactsTest {
    private val stored = mutableMapOf<String, String>()
    private val preferences = mockk<EncryptedPreferences> {
        every { putEncryptedString(any(), any()) } answers { stored[firstArg()] = secondArg() }
        every { getDecryptedString(any()) } answers { stored[firstArg()] }
    }
    private val bob = ByteArray(32) { 0xB0.toByte() }
    private val carol = ByteArray(32) { 0xC0.toByte() }

    private fun contacts() = ProductChatContacts(preferences)

    @Test
    fun `the chat product's contacts survive a restart and open its rooms`() = runTest {
        contacts().replace(
            "chat.paseo",
            listOf(
                HostChatContact(bob, "bob.01", "dm-b0", blocked = false),
                HostChatContact(carol, null, "dm-c0", blocked = true),
            ),
        )
        contacts().replace("other.paseo", listOf(HostChatContact(bob, "", "r", blocked = false)))

        val room = { id: String -> ChatId.forExtensionRoom("ProductBot_chat.paseo", id) }
        assertEquals(
            listOf(
                PersonChatContact(DataByteArray(bob), "bob.01", room("dm-b0"), blocked = false),
                PersonChatContact(DataByteArray(carol), null, room("dm-c0"), blocked = true),
                PersonChatContact(DataByteArray(bob), "", ChatId.forExtensionRoom("ProductBot_other.paseo", "r"), blocked = false),
            ),
            contacts().observeContacts().first(),
        )
    }

    @Test
    fun `a product's new list replaces only its own contacts`() = runTest {
        val store = contacts()
        store.replace("chat.paseo", listOf(HostChatContact(bob, "bob.01", "dm-b0", blocked = false)))
        store.replace("other.paseo", listOf(HostChatContact(carol, "carol", "r", blocked = false)))

        store.replace("chat.paseo", emptyList())

        assertEquals(listOf("carol"), contacts().getContacts().map { it.username })
    }
}
