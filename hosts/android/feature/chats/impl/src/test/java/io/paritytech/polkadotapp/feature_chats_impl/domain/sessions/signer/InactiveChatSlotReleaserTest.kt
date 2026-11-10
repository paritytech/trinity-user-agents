package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOf
import org.junit.Assert.assertEquals
import org.junit.Test

class InactiveChatSlotReleaserTest {
    private val active = contact(seed = 1)
    private val blocked = contact(seed = 2, isBlocked = true)
    private val pending = contact(seed = 3, pendingChatRequestId = "request")

    private val contactsRepository: ContactsRepository = mockk()
    private val accountDerivation: ChatSignerAccountDerivation = mockk()
    private val slotAllocator = RecordingSlotAllocator()

    private val releaser = InactiveChatSlotReleaser(contactsRepository, accountDerivation, slotAllocator)

    @Test
    fun `releases the slots of chats that are blocked, pending or gone and keeps the active ones`() {
        val leftover = chatAccount(seed = 9)
        withContacts(active, blocked, pending)
        slotAllocator.chatSignerTargets = setOf(chatAccountOf(active), chatAccountOf(blocked), leftover)

        releaseInactive()

        assertEquals(setOf(chatAccountOf(blocked), leftover), slotAllocator.deallocatedTargets.toSet())
    }

    @Test
    fun `keeps every slot when the active chats cannot all be derived`() {
        withContacts(active)
        slotAllocator.chatSignerTargets = setOf(chatAccount(seed = 9))
        val activeAccountId = active.accountId
        coEvery { accountDerivation.deriveAccountId(activeAccountId) } returns Result.failure(IllegalStateException())

        releaseInactive()

        assertEquals(emptyList<AccountId>(), slotAllocator.deallocatedTargets)
    }

    private fun withContacts(vararg contacts: Contact) {
        every { contactsRepository.subscribeContacts() } returns flowOf(contacts.toList())
        coEvery { contactsRepository.getContacts() } returns contacts.toList()
        contacts.forEach { contact ->
            val contactAccountId = contact.accountId
            val chatAccountId = chatAccountOf(contact)
            coEvery { accountDerivation.deriveAccountId(contactAccountId) } returns Result.success(chatAccountId)
        }
    }

    private fun releaseInactive() {
        with(UnconfinedComputationalScope()) { releaser.initialize() }
    }

    private fun chatAccountOf(contact: Contact): AccountId = chatAccount(contact.accountId.value.first())

    private fun chatAccount(seed: Byte): AccountId = ByteArray(32) { (seed + 100).toByte() }.toDataByteArray()

    private fun contact(seed: Byte, isBlocked: Boolean = false, pendingChatRequestId: String? = null): Contact = mockk {
        every { accountId } returns ByteArray(32) { seed }.toDataByteArray()
        every { this@mockk.isBlocked } returns isBlocked
        every { isPeerLeft } returns false
        every { this@mockk.pendingChatRequestId } returns pendingChatRequestId
    }

    private class UnconfinedComputationalScope : ComputationalScope, CoroutineScope by CoroutineScope(Dispatchers.Unconfined)
}
