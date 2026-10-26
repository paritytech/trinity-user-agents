package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningConsentRequired
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningGate
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationAllocationError
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementAccountAllocator
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ChatRequestDeliveryAccountResolverTest {
    private val period = 20_000u
    private val contactAccountId = ByteArray(32) { 9 }.toDataByteArray()
    private val contact: Contact = mockk { every { accountId } returns contactAccountId }
    private val request = outgoingRequest("request", ChatRequest.Delivery.Undelivered)
    private val account = deliveryAccount(seed = 1, period)
    private val accountId = account.accountId
    private val usernameSigner = deliverySigner(ChatRequest.Delivery.DeliveredByUsername(period))
    private val anonymousSigner = deliverySigner(ChatRequest.Delivery.DeliveredAnonymously(period))

    private val allocator: NotificationStatementAccountAllocator = mockk()
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation = mockk()
    private val signers: ChatRequestDeliverySigners = mockk()
    private val identitySigningGate: IdentitySigningGate = mockk()

    private val resolver = ChatRequestDeliveryAccountResolver(allocator, keypairDerivation, signers, identitySigningGate)

    @Before
    fun setUp() {
        every { allocator.currentPeriod() } returns period
        coEvery { keypairDerivation.deliveryAccount(request.id, period) } returns Result.success(account)
        coEvery { signers.usernameSigner(contact, period) } returns Result.success(usernameSigner)
        every { signers.anonymousSigner(account) } returns anonymousSigner
    }

    @Test
    fun `signs with the period account once its slot is claimed`() = runBlocking<Unit> {
        withAllocation(Result.success(Unit))

        val signer = resolver.resolveFirstDelivery(contact, request).getOrThrow()

        assertEquals(ChatRequest.Delivery.DeliveredAnonymously(period), signer.delivery)
    }

    @Test
    fun `falls back to our own account when no slot is free and the user allowed it`() = runBlocking<Unit> {
        withAllocation(Result.failure(NotificationAllocationError.NoFreeSlotInPeriod(accountId)))
        withConsent(Result.success(Unit))

        assertSame(usernameSigner, resolver.resolveFirstDelivery(contact, request).getOrThrow())
    }

    @Test
    fun `waits for the user instead of signing with our own account without consent`() = runBlocking<Unit> {
        withAllocation(Result.failure(NotificationAllocationError.NoFreeSlotInPeriod(accountId)))
        withConsent(Result.failure(IdentitySigningConsentRequired(contactAccountId)))

        val error = resolver.resolveFirstDelivery(contact, request).exceptionOrNull()

        assertTrue(error is IdentitySigningConsentRequired)
        coVerify(exactly = 0) { signers.usernameSigner(any(), any()) }
    }

    @Test
    fun `re-sending our own copy without consent signs nothing and asks nothing`() = runBlocking<Unit> {
        coEvery { identitySigningGate.hasConsent(contactAccountId) } returns false

        val signer = resolver.consentedUsernameSigner(contact, period).getOrThrow()

        assertEquals(null, signer)
        coVerify(exactly = 0) { identitySigningGate.ensureConsent(any()) }
    }

    @Test
    fun `signs an acceptance with the period acceptance account`() = runBlocking<Unit> {
        val acceptanceAccount = deliveryAccount(seed = 4, period)
        val acceptanceAccountId = acceptanceAccount.accountId
        coEvery { keypairDerivation.acceptanceAccount(request.id, period) } returns Result.success(acceptanceAccount)
        every { signers.anonymousSigner(acceptanceAccount) } returns anonymousSigner
        coEvery { allocator.initiateAllocation(acceptanceAccountId) } returns Result.success(Unit)
        coEvery { allocator.awaitAllocated(acceptanceAccountId, any()) } returns Result.success(Unit)

        assertSame(anonymousSigner, resolver.resolveAcceptance(contact, request.id).getOrThrow())
    }

    @Test
    fun `fails instead of falling back when the claim did not land in time`() = runBlocking<Unit> {
        withAllocation(Result.failure(NotificationAllocationError.Timeout(accountId, kotlin.time.Duration.ZERO)))

        val error = resolver.resolveFirstDelivery(contact, request).exceptionOrNull()

        assertTrue(error is NotificationAllocationError.Timeout)
    }

    private fun withConsent(result: Result<Unit>) {
        coEvery { identitySigningGate.ensureConsent(contactAccountId) } returns result
    }

    private fun withAllocation(result: Result<Unit>) {
        coEvery { allocator.initiateAllocation(accountId) } returns Result.success(Unit)
        coEvery { allocator.awaitAllocated(accountId, any()) } returns result
    }
}
