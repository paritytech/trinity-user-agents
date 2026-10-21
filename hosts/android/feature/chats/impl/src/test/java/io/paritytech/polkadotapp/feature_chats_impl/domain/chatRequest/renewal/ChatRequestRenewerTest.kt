package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.renewal

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestPayload
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccount
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryKeypairDerivation
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigners
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestPayloadLoader
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.deliveryAccount
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.outgoingRequest
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementAccountAllocator
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.Test

class ChatRequestRenewerTest {
    private val period = 20_000u
    private val contact: Contact = mockk()
    private val payload: OutgoingChatRequestPayload = mockk()

    private val contactsRepository: ContactsRepository = mockk()
    private val allocator: NotificationStatementAccountAllocator = mockk()
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation = mockk()
    private val outgoingService: OutgoingChatRequestService = mockk()
    private val payloadLoader: ChatRequestPayloadLoader = mockk()
    private val publisher: ChatRequestPublisher = mockk()
    private val signers: ChatRequestDeliverySigners = mockk()

    private val renewer = ChatRequestRenewer(
        contactsRepository, allocator, keypairDerivation, outgoingService, payloadLoader, publisher, signers,
    )

    @Before
    fun setUp() {
        every { allocator.currentPeriod() } returns period
        coEvery { payloadLoader.load(contact, any()) } returns Result.success(payload)
        coEvery { publisher.republish(any(), payload, any()) } returns Result.success(Unit)
    }

    @Test
    fun `renews a request of an earlier period from a fresh account`() = runBlocking<Unit> {
        val request = withPendingRequest("stale", deliveredPeriod = period - 1u)
        val fresh = withDeliveryAccount(request, period, seed = 2)
        withClaimed(fresh)
        val freshId = fresh.accountId
        coEvery { allocator.awaitAllocated(freshId, any()) } returns Result.success(Unit)

        renewer.renew().getOrThrow()

        verifyPublishedFrom(request, fresh)
    }

    @Test
    fun `leaves a stale request on its old copy when no slot was claimed for it`() = runBlocking<Unit> {
        val request = withPendingRequest("stale", deliveredPeriod = period - 1u)
        withDeliveryAccount(request, period, seed = 2)
        withClaimed()

        renewer.renew().getOrThrow()

        verifyNothingPublished()
    }

    @Test
    fun `re-sends a request of this period that went missing from the store`() = runBlocking<Unit> {
        val request = withPendingRequest("current", deliveredPeriod = period)
        val account = withDeliveryAccount(request, period, seed = 3)
        withStored(account, stored = false)

        renewer.renew().getOrThrow()

        verifyPublishedFrom(request, account)
    }

    @Test
    fun `leaves a request of this period alone while it is still stored`() = runBlocking<Unit> {
        val request = withPendingRequest("current", deliveredPeriod = period)
        val account = withDeliveryAccount(request, period, seed = 3)
        withStored(account, stored = true)

        renewer.renew().getOrThrow()

        verifyNothingPublished()
    }

    private fun withPendingRequest(id: String, deliveredPeriod: UInt): ChatRequest {
        val request = outgoingRequest(id, ChatRequest.Delivery.DeliveredAnonymously(deliveredPeriod))
        coEvery { contactsRepository.getContactsWithAnonymouslyDeliveredPendingRequests() } returns
            listOf(ContactWithChatRequest(contact, request))
        return request
    }

    private fun withDeliveryAccount(request: ChatRequest, period: UInt, seed: Byte): ChatRequestDeliveryAccount {
        val account = deliveryAccount(seed, period)
        coEvery { keypairDerivation.deliveryAccount(request.id, period) } returns Result.success(account)
        every { signers.anonymousSigner(account) } returns ChatRequestDeliverySigner(mockk(), ChatRequest.Delivery.DeliveredAnonymously(period))
        return account
    }

    private fun withClaimed(vararg accounts: ChatRequestDeliveryAccount) {
        val claimed = accounts.map { it.accountId }
        coEvery { allocator.initiateAllocations(any()) } returns Result.success(claimed)
    }

    private fun withStored(account: ChatRequestDeliveryAccount, stored: Boolean) {
        val accountId = account.accountId
        coEvery { outgoingService.isStoredBy(contact, accountId) } returns Result.success(stored)
    }

    private fun verifyPublishedFrom(request: ChatRequest, account: ChatRequestDeliveryAccount) {
        val period = account.period
        coVerify(exactly = 1) {
            publisher.republish(request, payload, match { it.delivery == ChatRequest.Delivery.DeliveredAnonymously(period) })
        }
    }

    private fun verifyNothingPublished() {
        coVerify(exactly = 0) { publisher.republish(any(), any(), any()) }
    }
}
