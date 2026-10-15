package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestPayload
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.Test

class ChatRequestDeliveryServiceTest {
    private val contactAccountId = ByteArray(32) { 9 }.toDataByteArray()
    private val contact: Contact = mockk()
    private val request = outgoingRequest("request", ChatRequest.Delivery.Undelivered)
    private val payload: OutgoingChatRequestPayload = mockk()
    private val signer = ChatRequestDeliverySigner(mockk(), ChatRequest.Delivery.DeliveredAnonymously(1u))

    private val contactsRepository: ContactsRepository = mockk()
    private val chatRequestRepository: ChatRequestRepository = mockk()
    private val payloadLoader: ChatRequestPayloadLoader = mockk()
    private val publisher: ChatRequestPublisher = mockk()
    private val accountResolver: ChatRequestDeliveryAccountResolver = mockk()
    private val connectionRefCounter: ChainConnectionRefCounter = mockk()
    private val knownChains: KnownChains = mockk { every { people } returns "people" }
    private val dispatchers: CoroutineDispatchers = mockk { every { computation } returns Dispatchers.Unconfined }

    private val service = ChatRequestDeliveryService(
        contactsRepository, chatRequestRepository, payloadLoader, publisher, accountResolver,
        connectionRefCounter, knownChains, dispatchers,
    )

    @Before
    fun setUp() {
        every { contactsRepository.subscribeContactsWithUndeliveredOutgoingRequests() } returns
            flowOf(listOf(ContactWithChatRequest(contact, request)))
        coEvery { chatRequestRepository.getById(request.id) } returns request
        coEvery { payloadLoader.load(contact, request) } returns Result.success(payload)
        coEvery { connectionRefCounter.requestConnectionEnabled(any(), any()) } returns mockk(relaxed = true)
        every { contact.accountId } returns contactAccountId
    }

    @Test
    fun `publishes a fitting request as its first delivery`() = runBlocking<Unit> {
        withContactLinkedTo(request.id)
        withRequestFitting(true)
        coEvery { accountResolver.resolveFirstDelivery(contact, request) } returns Result.success(signer)
        coEvery { publisher.publishFirstDelivery(request, payload, signer) } returns Result.success(Unit)

        startDelivery()

        coVerify { publisher.publishFirstDelivery(request, payload, signer) }
    }

    @Test
    fun `does not publish a request the contact no longer points at`() = runBlocking<Unit> {
        withContactLinkedTo(null)

        startDelivery()

        coVerify(exactly = 0) { payloadLoader.load(any(), any()) }
    }

    @Test
    fun `marks an oversized request undeliverable without claiming a slot`() = runBlocking<Unit> {
        withContactLinkedTo(request.id)
        withRequestFitting(false)
        coEvery { publisher.markUndeliverable(request) } returns Result.success(Unit)

        startDelivery()

        coVerify { publisher.markUndeliverable(request) }
        coVerify(exactly = 0) { accountResolver.resolveFirstDelivery(any(), any()) }
    }

    private fun withContactLinkedTo(requestId: String?) {
        val stored: Contact = mockk { every { pendingChatRequestId } returns requestId }
        coEvery { contactsRepository.getContact(contactAccountId) } returns stored
    }

    private fun withRequestFitting(fits: Boolean) {
        coEvery { publisher.fitsNotificationStatement(request, payload) } returns Result.success(fits)
    }

    private fun startDelivery() {
        with(UnconfinedComputationalScope()) { service.initialize() }
    }

    private class UnconfinedComputationalScope : ComputationalScope, CoroutineScope by CoroutineScope(Dispatchers.Unconfined)
}
