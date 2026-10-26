package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningConsentRequired
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningGate
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocationError
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ContactChatSignerResolverTest {
    private val ourMetaAccountId = 7L
    private val contactAccountId = ByteArray(32) { 1 }.toDataByteArray()
    private val chatPublicKey = ByteArray(32) { 2 }
    private val chatAccountId = chatPublicKey.toDataByteArray()
    private val chatKeypair: Sr25519Keypair = mockk { every { publicKey } returns chatPublicKey }
    private val usernameKeypair: Sr25519Keypair = mockk()
    private val contact: Contact = mockk {
        every { accountId } returns contactAccountId
        every { ourMetaAccountId } returns this@ContactChatSignerResolverTest.ourMetaAccountId
        every { username } returns "alice"
    }

    private val accountDerivation: ChatSignerAccountDerivation = mockk()
    private val slotAllocator = RecordingSlotAllocator()
    private val accountSecretsStorage: AccountSecretsStorage = mockk()
    private val connectionRefCounter: ChainConnectionRefCounter = mockk()
    private val knownChains: KnownChains = mockk { every { people } returns "people" }
    private val identitySigningGate: IdentitySigningGate = mockk()

    private val resolver = ContactChatSignerResolver(
        accountDerivation, slotAllocator, accountSecretsStorage, connectionRefCounter, knownChains, identitySigningGate,
    )

    @Before
    fun setUp() {
        coEvery { accountDerivation.deriveKeypair(contactAccountId) } returns Result.success(chatKeypair)
        coEvery { accountSecretsStorage.getMetaAccountKeypair(ourMetaAccountId) } returns usernameKeypair
        coEvery { connectionRefCounter.requestConnectionEnabled(any(), any()) } returns mockk(relaxed = true)
    }

    @Test
    fun `slot held this period resolves to private signer without allocating`() = runBlocking<Unit> {
        withCurrentAllocation(true)

        val signer = resolver.resolve(contact).getOrNull()

        assertPrivateWith(signer, chatKeypair)
        verifyNoAllocation()
    }

    @Test
    fun `missing slot is allocated as Critical and resolves to private signer`() = runBlocking<Unit> {
        withCurrentAllocation(false)
        withAllocationResult(Result.success(Unit))

        val signer = resolver.resolve(contact).getOrNull()

        assertPrivateWith(signer, chatKeypair)
        verifyCriticalAllocationRequested()
    }

    @Test
    fun `full slot table falls back to username signer once the user allowed it`() = runBlocking<Unit> {
        withCurrentAllocation(false)
        withAllocationResult(Result.failure(StatementStoreSlotAllocationError.NoAllocationAvailable(IllegalStateException())))
        coEvery { identitySigningGate.ensureConsent(contactAccountId) } returns Result.success(Unit)

        val signer = resolver.resolve(contact).getOrNull()

        assertEquals(ChatSignerKind.USERNAME, signer?.kind)
        assertSame(usernameKeypair, signer?.keypair)
        assertEquals(slotAllocator.period, signer?.period)
    }

    @Test
    fun `full slot table without the user's consent is returned instead of downgrading`() = runBlocking<Unit> {
        withCurrentAllocation(false)
        withAllocationResult(Result.failure(StatementStoreSlotAllocationError.NoAllocationAvailable(IllegalStateException())))
        coEvery { identitySigningGate.ensureConsent(contactAccountId) } returns
            Result.failure(IdentitySigningConsentRequired(contactAccountId))

        val error = resolver.resolve(contact).exceptionOrNull()

        assertTrue(error is IdentitySigningConsentRequired)
    }

    @Test
    fun `transient allocation failure is returned instead of downgrading`() = runBlocking<Unit> {
        withCurrentAllocation(false)
        withAllocationResult(Result.failure(StatementStoreSlotAllocationError.Unknown(IllegalStateException())))

        val result = resolver.resolve(contact)

        assertTrue(result.isFailure)
    }

    private fun withCurrentAllocation(hasCurrent: Boolean) {
        slotAllocator.hasCurrentAllocation = hasCurrent
    }

    private fun withAllocationResult(result: Result<Unit>) {
        slotAllocator.allocationResult = result
    }

    private fun verifyCriticalAllocationRequested() {
        val call = slotAllocator.allocateCalls.single()
        assertEquals(chatAccountId, call.target)
        assertEquals(OnExistingAllocationStrategy.IGNORE, call.strategy)
        assertEquals(SlotPriority.Critical, call.priority)
        assertEquals(CHAT_SIGNER_SLOT_TAG, call.tag)
    }

    private fun verifyNoAllocation() {
        assertTrue(slotAllocator.allocateCalls.isEmpty())
    }

    private fun assertPrivateWith(signer: ContactChatSigner?, keypair: Sr25519Keypair) {
        assertEquals(ChatSignerKind.PRIVATE, signer?.kind)
        assertSame(keypair, signer?.keypair)
    }
}
