package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.renewal

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestPayload
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccount
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccountResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryKeypairDerivation
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigners
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestPayloadLoader
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.deliveryAccount
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.deliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.outgoingRequest
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningConsentRequired
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningDeclined
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementAccountAllocator
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.Test

class ChatRequestRenewerTest {
    private val period = 20_000u
    private val contactAccountId = ByteArray(32) { 8 }.toDataByteArray()
    private val contact: Contact = mockk()
    private val payload: OutgoingChatRequestPayload = mockk()
    private val usernameSigner = deliverySigner(ChatRequest.Delivery.DeliveredByUsername(period), seed = 9)

    private val contactsRepository: ContactsRepository = mockk()
    private val allocator: NotificationStatementAccountAllocator = mockk()
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation = mockk()
    private val outgoingService: OutgoingChatRequestService = mockk()
    private val payloadLoader: ChatRequestPayloadLoader = mockk()
    private val publisher: ChatRequestPublisher = mockk()
    private val signers: ChatRequestDeliverySigners = mockk()
    private val accountResolver: ChatRequestDeliveryAccountResolver = mockk()

    private val renewer = ChatRequestRenewer(
        contactsRepository, keypairDerivation, outgoingService, payloadLoader, publisher,
        OneShotDeliveryRenewer(allocator, signers, accountResolver),
    )

    @Before
    fun setUp() {
        every { allocator.currentPeriod() } returns period
        coEvery { payloadLoader.load(contact, any()) } returns Result.success(payload)
        coEvery { publisher.republish(any(), payload, any()) } returns Result.success(Unit)
        coEvery { publisher.markFailed(any()) } returns Result.success(Unit)
        withOurAccountAnswer(Result.failure(IdentitySigningConsentRequired(contactAccountId)))
    }

    @Test
    fun `renews a request of an earlier period from a fresh account`() = runBlocking<Unit> {
        val request = withPendingRequest("stale", ChatRequest.Delivery.DeliveredAnonymously(period - 1u))
        val fresh = withDeliveryAccount(request, period, seed = 2)
        withClaimedAndAllocated(fresh)

        renewer.renew().getOrThrow()

        verifyPublishedFrom(request, fresh)
    }

    @Test
    fun `a request our own account signed goes back to a fresh account once a slot is free`() = runBlocking<Unit> {
        val request = withPendingRequest("stale", ChatRequest.Delivery.DeliveredByUsername(period - 1u))
        val fresh = withDeliveryAccount(request, period, seed = 2)
        withClaimedAndAllocated(fresh)

        renewer.renew().getOrThrow()

        verifyPublishedFrom(request, fresh)
    }

    @Test
    fun `leaves a stale request on its old copy while the user has not answered`() = runBlocking<Unit> {
        val request = withPendingRequest("stale", ChatRequest.Delivery.DeliveredAnonymously(period - 1u))
        withDeliveryAccount(request, period, seed = 2)
        withClaimed()

        renewer.renew().getOrThrow()

        verifyNothingPublished()
        coVerify(exactly = 0) { publisher.markFailed(any()) }
    }

    @Test
    fun `renews a stale request from our own account when no slot was claimed and the user allowed it`() = runBlocking<Unit> {
        val request = withPendingRequest("stale", ChatRequest.Delivery.DeliveredAnonymously(period - 1u))
        withDeliveryAccount(request, period, seed = 2)
        withClaimed()
        withOurAccountAnswer(Result.success(usernameSigner))

        renewer.renew().getOrThrow()

        coVerify(exactly = 1) { publisher.republish(request, payload, usernameSigner) }
    }

    @Test
    fun `fails a stale request without a slot once the user declined our own account`() = runBlocking<Unit> {
        val request = withPendingRequest("stale", ChatRequest.Delivery.DeliveredAnonymously(period - 1u))
        withDeliveryAccount(request, period, seed = 2)
        withClaimed()
        withOurAccountAnswer(Result.failure(IdentitySigningDeclined(contactAccountId)))

        renewer.renew().getOrThrow()

        verifyNothingPublished()
        coVerify(exactly = 1) { publisher.markFailed(request) }
    }

    @Test
    fun `re-sends a request of this period that went missing from the store`() = runBlocking<Unit> {
        val request = withPendingRequest("current", ChatRequest.Delivery.DeliveredAnonymously(period))
        val account = withDeliveryAccount(request, period, seed = 3)
        withStored(account.accountId, stored = false)

        renewer.renew().getOrThrow()

        verifyPublishedFrom(request, account)
    }

    @Test
    fun `leaves a request of this period alone while it is still stored`() = runBlocking<Unit> {
        val request = withPendingRequest("current", ChatRequest.Delivery.DeliveredAnonymously(period))
        val account = withDeliveryAccount(request, period, seed = 3)
        withStored(account.accountId, stored = true)

        renewer.renew().getOrThrow()

        verifyNothingPublished()
    }

    @Test
    fun `re-sends our own copy of this period that went missing while the user still allows it`() = runBlocking<Unit> {
        val request = withPendingRequest("current", ChatRequest.Delivery.DeliveredByUsername(period))
        coEvery { accountResolver.consentedUsernameSigner(contact, period) } returns Result.success(usernameSigner)
        withStored(usernameSigner.accountId, stored = false)

        renewer.renew().getOrThrow()

        coVerify(exactly = 1) { publisher.republish(request, payload, usernameSigner) }
    }

    @Test
    fun `leaves our own copy of this period alone once the user no longer allows it`() = runBlocking<Unit> {
        withPendingRequest("current", ChatRequest.Delivery.DeliveredByUsername(period))
        coEvery { accountResolver.consentedUsernameSigner(contact, period) } returns Result.success(null)

        renewer.renew().getOrThrow()

        verifyNothingPublished()
    }

    private fun withPendingRequest(id: String, delivery: ChatRequest.Delivery): ChatRequest {
        val request = outgoingRequest(id, delivery)
        coEvery { contactsRepository.getContactsWithRenewablePendingRequests() } returns
            listOf(ContactWithChatRequest(contact, request))
        return request
    }

    private fun withDeliveryAccount(request: ChatRequest, period: UInt, seed: Byte): ChatRequestDeliveryAccount {
        val account = deliveryAccount(seed, period)
        val signer = ChatRequestDeliverySigner(mockk(), account.accountId, ChatRequest.Delivery.DeliveredAnonymously(period))
        coEvery { keypairDerivation.deliveryAccount(request.id, period) } returns Result.success(account)
        every { signers.anonymousSigner(account) } returns signer
        return account
    }

    private fun withOurAccountAnswer(result: Result<ChatRequestDeliverySigner>) {
        coEvery { accountResolver.usernameSignerIfAllowed(contact, period) } returns result
    }

    private fun withClaimed(vararg accounts: ChatRequestDeliveryAccount) {
        val claimed = accounts.map { it.accountId }
        coEvery { allocator.initiateAllocations(any()) } returns Result.success(claimed)
    }

    private fun withClaimedAndAllocated(account: ChatRequestDeliveryAccount) {
        withClaimed(account)
        val accountId = account.accountId
        coEvery { allocator.awaitAllocated(accountId, any()) } returns Result.success(Unit)
    }

    private fun withStored(signer: AccountId, stored: Boolean) {
        coEvery { outgoingService.isStoredBy(contact, signer) } returns Result.success(stored)
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
