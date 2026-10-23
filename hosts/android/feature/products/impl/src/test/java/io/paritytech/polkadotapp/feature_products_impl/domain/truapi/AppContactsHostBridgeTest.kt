package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContact
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.HostContactLookup

/**
 * The bridge answers lookups from the latest contact snapshot and a handle
 * index per key. These pin that the index follows the snapshot and the key, so
 * a lookup never answers from contacts or a session that are gone.
 */
class AppContactsHostBridgeTest {
    private val key = ByteArray(32) { 0x11 }
    private val otherKey = ByteArray(32) { 0x12 }
    private val alice = ByteArray(32) { 0xA1.toByte() }
    private val bob = ByteArray(32) { 0xB0.toByte() }
    private val bridge = AppContactsHostBridge(mockk<PersonChatContacts>(), mockk(relaxed = true))

    @Test
    fun `a handle resolves to the contact it hashes from`() {
        bridge.update(listOf(contact(alice), contact(bob)))

        val matches = bridge.contacts(HostContactLookup(key, listOf(contactHandle(key, bob), ByteArray(32))))

        assertArrayEquals(bob, matches.accounts[0])
        assertNull(matches.accounts[1])
    }

    @Test
    fun `a new snapshot drops a removed contact from the index`() {
        bridge.update(listOf(contact(alice)))
        bridge.contacts(HostContactLookup(key, listOf(contactHandle(key, alice))))

        bridge.update(emptyList())
        val matches = bridge.contacts(HostContactLookup(key, listOf(contactHandle(key, alice))))

        assertEquals(listOf(null), matches.accounts)
    }

    @Test
    fun `a new handle key is hashed afresh`() {
        bridge.update(listOf(contact(alice)))
        bridge.contacts(HostContactLookup(key, listOf(contactHandle(key, alice))))

        val matches = bridge.contacts(
            HostContactLookup(otherKey, listOf(contactHandle(key, alice), contactHandle(otherKey, alice))),
        )

        assertNull(matches.accounts[0])
        assertArrayEquals(alice, matches.accounts[1])
    }

    @Test
    fun `the picker offers the chat product's unblocked contacts, each person once`() = runBlocking {
        val carol = ByteArray(32) { 0xC0.toByte() }
        val personChat = mockk<PersonChatContacts> {
            coEvery { getContacts() } returns listOf(
                productContact(alice, "alice.product", blocked = false),
                productContact(bob, "bob", blocked = false),
                productContact(carol, "carol", blocked = true),
                productContact(bob, "bob", blocked = false),
            )
        }
        val request = slot<ContactPickRequest>()
        val picks = mockk<TrUAPIContactPicks> { coEvery { ask(capture(request)) } returns null }

        AppContactsHostBridge(personChat, picks).pickContact("notes.dot")

        assertEquals(
            listOf(DataByteArray(alice) to "alice.product", DataByteArray(bob) to "bob"),
            request.captured.options.map { DataByteArray(it.account) to it.displayName },
        )
    }

    // Lookups answer from what the chat product syncs: before any snapshot they read it, and a blocked contact is gone.
    @Test
    fun `before a snapshot a handle resolves through the chat product's contacts`() {
        val personChat = mockk<PersonChatContacts> {
            coEvery { getContacts() } returns listOf(productContact(alice, "alice", blocked = false), productContact(bob, "bob", blocked = true))
        }
        val matches = AppContactsHostBridge(personChat, mockk(relaxed = true))
            .contacts(HostContactLookup(key, listOf(contactHandle(key, alice), contactHandle(key, bob))))

        assertArrayEquals(alice, matches.accounts[0])
        assertNull(matches.accounts[1])
    }

    @Test
    fun `a contact the chat product drops or blocks is a removal`() = runBlocking {
        val synced = MutableStateFlow(listOf(productContact(alice, "alice", blocked = false), productContact(bob, "bob", blocked = false)))
        val personChat = mockk<PersonChatContacts> { every { observeContacts() } returns synced }
        val removals = mutableListOf<Unit>()
        val collection = launch(Dispatchers.Unconfined) { AppContactsHostBridge(personChat, mockk(relaxed = true)).contactRemovals().collect { removals += it } }

        synced.value = synced.value + productContact(ByteArray(32) { 0xC0.toByte() }, "carol", blocked = false)
        assertEquals(0, removals.size)
        synced.value = listOf(productContact(alice, "alice", blocked = false), productContact(bob, "bob", blocked = true))
        assertEquals(1, removals.size)
        collection.cancel()
    }

    private fun contact(account: ByteArray) = PickableContact(DataByteArray(account), username = null)

    private fun productContact(account: ByteArray, name: String, blocked: Boolean) =
        PersonChatContact(DataByteArray(account), name, ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-x"), blocked)
}
