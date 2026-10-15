package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
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
    private val contact: Contact = mockk()
    private val request = outgoingRequest("request", ChatRequest.Delivery.Undelivered)
    private val account = deliveryAccount(seed = 1, period)
    private val accountId = account.accountId
    private val usernameSigner = ChatRequestDeliverySigner(mockk(), ChatRequest.Delivery.Delivered)
    private val anonymousSigner = ChatRequestDeliverySigner(mockk(), ChatRequest.Delivery.DeliveredAnonymously(period))

    private val allocator: NotificationStatementAccountAllocator = mockk()
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation = mockk()
    private val signers: ChatRequestDeliverySigners = mockk()

    private val resolver = ChatRequestDeliveryAccountResolver(allocator, keypairDerivation, signers)

    @Before
    fun setUp() {
        every { allocator.currentPeriod() } returns period
        coEvery { keypairDerivation.deliveryAccount(request.id, period) } returns Result.success(account)
        coEvery { signers.usernameSigner(contact) } returns Result.success(usernameSigner)
        every { signers.anonymousSigner(account) } returns anonymousSigner
    }

    @Test
    fun `signs with the period account once its slot is claimed`() = runBlocking<Unit> {
        withAllocation(Result.success(Unit))

        val signer = resolver.resolveFirstDelivery(contact, request).getOrThrow()

        assertEquals(ChatRequest.Delivery.DeliveredAnonymously(period), signer.delivery)
    }

    @Test
    fun `falls back to our own account when no slot is free`() = runBlocking<Unit> {
        withAllocation(Result.failure(NotificationAllocationError.NoFreeSlotInPeriod(accountId)))

        assertSame(usernameSigner, resolver.resolveFirstDelivery(contact, request).getOrThrow())
    }

    @Test
    fun `fails instead of falling back when the claim did not land in time`() = runBlocking<Unit> {
        withAllocation(Result.failure(NotificationAllocationError.Timeout(accountId, kotlin.time.Duration.ZERO)))

        val error = resolver.resolveFirstDelivery(contact, request).exceptionOrNull()

        assertTrue(error is NotificationAllocationError.Timeout)
    }

    private fun withAllocation(result: Result<Unit>) {
        coEvery { allocator.initiateAllocation(accountId) } returns Result.success(Unit)
        coEvery { allocator.awaitAllocated(accountId, any()) } returns result
    }
}
