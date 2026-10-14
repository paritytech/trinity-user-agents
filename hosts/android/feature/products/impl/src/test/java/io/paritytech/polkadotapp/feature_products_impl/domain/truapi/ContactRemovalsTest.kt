package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The core caches the contact handles it resolves, so the host has to say when
 * a cached one may have stopped naming a contact. These pin that it says so
 * exactly when somebody leaves the list the directory serves, which already
 * excludes blocked contacts.
 */
class ContactRemovalsTest {
    private val alice = contact(0xA1)
    private val bob = contact(0xB0)

    @Test
    fun `the first list is a baseline, not a change`() = runBlocking {
        assertEquals(0, removals(listOf(alice)))
    }

    @Test
    fun `a contact added does not clear the cache`() = runBlocking {
        assertEquals(0, removals(listOf(alice), listOf(alice, bob)))
    }

    @Test
    fun `a contact removed or blocked clears the cache`() = runBlocking {
        assertEquals(1, removals(listOf(alice, bob), listOf(alice)))
    }

    @Test
    fun `every removal is reported`() = runBlocking {
        assertEquals(2, removals(listOf(alice, bob), listOf(alice), listOf(alice, bob), listOf(bob)))
    }

    private suspend fun removals(vararg lists: List<Contact>): Int =
        flowOf(*lists).contactRemovals().toList().size

    private fun contact(byte: Int): Contact = mockk {
        every { accountId } returns DataByteArray(ByteArray(32) { byte.toByte() })
    }
}
