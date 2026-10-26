package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.acceptance

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatAcceptanceRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccountResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryKeypairDerivation
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigners
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.deliveryAccount
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.renewal.OneShotDeliveryRenewer
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementAccountAllocator
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.Test
import kotlin.time.Duration
import kotlin.time.Duration.Companion.days
import kotlin.time.Instant

class ChatAcceptanceRenewerTest {
    private val period = 20_000u
    private val now = Instant.fromEpochMilliseconds(100.days.inWholeMilliseconds)
    private val contactAccountId = ByteArray(32) { 6 }.toDataByteArray()
    private val contact: Contact = mockk()

    private val chatAcceptanceRepository: ChatAcceptanceRepository = mockk()
    private val contactsRepository: ContactsRepository = mockk()
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation = mockk()
    private val publisher: ChatAcceptancePublisher = mockk()
    private val allocator: NotificationStatementAccountAllocator = mockk()
    private val signers: ChatRequestDeliverySigners = mockk()
    private val accountResolver: ChatRequestDeliveryAccountResolver = mockk()

    private val renewer = ChatAcceptanceRenewer(
        chatAcceptanceRepository, contactsRepository, keypairDerivation, publisher,
        OneShotDeliveryRenewer(allocator, signers, accountResolver),
    )

    @Before
    fun setUp() {
        every { allocator.currentPeriod() } returns period
        coEvery { contactsRepository.getContact(contactAccountId) } returns contact
        coEvery { publisher.republish(contact, any(), any()) } returns Result.success(Unit)
    }

    @Test
    fun `renews an acceptance of an earlier period into the current one`() = runBlocking<Unit> {
        val acceptance = withPublishedAcceptance(acceptedAgo = 2.days)
        withFreshAccount(acceptance)

        renewer.renew().getOrThrow()

        coVerify(exactly = 1) { publisher.republish(contact, acceptance, any()) }
    }

    @Test
    fun `keeps renewing an acceptance the requester has ignored for over a week`() = runBlocking<Unit> {
        val acceptance = withPublishedAcceptance(acceptedAgo = 8.days)
        withFreshAccount(acceptance)

        renewer.renew().getOrThrow()

        coVerify(exactly = 1) { publisher.republish(contact, acceptance, any()) }
    }

    private fun withPublishedAcceptance(acceptedAgo: Duration): ChatAcceptance {
        val acceptance = ChatAcceptance(
            contactAccountId = contactAccountId,
            requestId = "request",
            messageId = "accepted-message",
            acceptedAt = now - acceptedAgo,
            delivery = ChatRequest.Delivery.DeliveredAnonymously(period - 1u),
        )
        coEvery { chatAcceptanceRepository.getDelivered() } returns listOf(acceptance)
        return acceptance
    }

    private fun withFreshAccount(acceptance: ChatAcceptance) {
        val account = deliveryAccount(seed = 2, period)
        val accountId = account.accountId
        coEvery { keypairDerivation.acceptanceAccount(acceptance.requestId, period) } returns Result.success(account)
        every { signers.anonymousSigner(account) } returns
            ChatRequestDeliverySigner(mockk(), accountId, ChatRequest.Delivery.DeliveredAnonymously(period))
        coEvery { allocator.initiateAllocations(any()) } returns Result.success(listOf(accountId))
        coEvery { allocator.awaitAllocated(accountId, any()) } returns Result.success(Unit)
    }
}
