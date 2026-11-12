package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDirectory
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.HostContactLookup
import uniffi.truapi.HostContactUsernames
import uniffi.truapi.ProductId

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
    private val carol = ByteArray(32) { 0xC0.toByte() }
    private val directory = mockk<ContactDirectory>()
    private val bridge = AppContactsHostBridge(directory, mockk(relaxed = true))

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
    fun `usernames leave out a contact without one and keep the label as stored`() {
        bridge.update(
            listOf(
                contact(alice, username = "alice.07"),
                contact(bob, username = null),
                contact(carol, username = ""),
            ),
        )

        val listing = bridge.contactUsernames(ProductId.parse("dim2.dot").getOrThrow())

        assertEquals(HostContactUsernames.Listed(listOf("alice.07")), listing)
    }

    @Test
    fun `usernames come from the directory when no snapshot has landed`() {
        coEvery { directory.getContacts() } returns listOf(
            contact(alice, username = "alice.07"),
            contact(carol, username = ""),
        )

        val listing = bridge.contactUsernames(ProductId.parse("dim2.dot").getOrThrow())

        assertEquals(HostContactUsernames.Listed(listOf("alice.07")), listing)
    }

    @Test
    fun `usernames follow the latest snapshot`() {
        bridge.update(listOf(contact(alice, username = "alice.07")))
        bridge.update(listOf(contact(bob, username = "bob.03")))

        val listing = bridge.contactUsernames(ProductId.parse("dim2.dot").getOrThrow())

        assertEquals(HostContactUsernames.Listed(listOf("bob.03")), listing)
    }

    private fun contact(account: ByteArray, username: String? = "test_user"): Contact = mockk {
        every { accountId } returns DataByteArray(account)
        every { this@mockk.username } returns username
    }
}
