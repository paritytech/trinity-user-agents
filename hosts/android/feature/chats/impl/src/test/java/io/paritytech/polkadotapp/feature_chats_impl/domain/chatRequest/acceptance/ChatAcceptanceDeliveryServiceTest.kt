package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.acceptance

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
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatAcceptanceRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccountResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.InvalidRequestIdJunction
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.deliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningDeclined
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningGate
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOf
import org.junit.Before
import org.junit.Test
import kotlin.time.Instant

class ChatAcceptanceDeliveryServiceTest {
    private val contactAccountId = ByteArray(32) { 5 }.toDataByteArray()
    private val contact: Contact = mockk()
    private val acceptedAt = Instant.fromEpochMilliseconds(1_000)
    private val acceptance = ChatAcceptance(contactAccountId, "request", "accepted-message", acceptedAt, ChatRequest.Delivery.Undelivered)
    private val signer = deliverySigner(ChatRequest.Delivery.DeliveredAnonymously(1u))

    private val chatAcceptanceRepository: ChatAcceptanceRepository = mockk()
    private val contactsRepository: ContactsRepository = mockk()
    private val accountResolver: ChatRequestDeliveryAccountResolver = mockk()
    private val publisher: ChatAcceptancePublisher = mockk()
    private val identitySigningGate: IdentitySigningGate = mockk(relaxed = true)
    private val connectionRefCounter: ChainConnectionRefCounter = mockk()
    private val knownChains: KnownChains = mockk { every { people } returns "people" }
    private val dispatchers: CoroutineDispatchers = mockk { every { computation } returns Dispatchers.Unconfined }

    private val service = ChatAcceptanceDeliveryService(
        chatAcceptanceRepository, contactsRepository, accountResolver, publisher, identitySigningGate,
        connectionRefCounter, knownChains, dispatchers,
    )

    @Before
    fun setUp() {
        every { chatAcceptanceRepository.subscribeUndelivered() } returns flowOf(listOf(acceptance))
        coEvery { contactsRepository.getContact(contactAccountId) } returns contact
        coEvery { connectionRefCounter.requestConnectionEnabled(any(), any()) } returns mockk(relaxed = true)
    }

    @Test
    fun `publishes a recorded acceptance from its period account`() {
        coEvery { chatAcceptanceRepository.get(contactAccountId) } returns acceptance
        coEvery { accountResolver.resolveAcceptance(contact, "request") } returns Result.success(signer)
        coEvery { publisher.publishFirstDelivery(contact, acceptance, signer) } returns Result.success(Unit)

        startDelivery()

        coVerify(exactly = 1) { publisher.publishFirstDelivery(contact, acceptance, signer) }
    }

    @Test
    fun `does not publish an acceptance a newer one replaced`() {
        val newer = ChatAcceptance(contactAccountId, "request", "newer-message", acceptedAt, ChatRequest.Delivery.Undelivered)
        coEvery { chatAcceptanceRepository.get(contactAccountId) } returns newer

        startDelivery()

        coVerify(exactly = 0) { accountResolver.resolveAcceptance(any(), any()) }
    }

    @Test
    fun `keeps retrying an acceptance whose request id cannot be derived from, as iOS does`() {
        val error = InvalidRequestIdJunction("request")
        coEvery { chatAcceptanceRepository.get(contactAccountId) } returns acceptance
        coEvery { accountResolver.resolveAcceptance(contact, "request") } returns Result.failure(error)

        startDelivery()

        coVerify(exactly = 1) { accountResolver.resolveAcceptance(contact, "request") }
        coVerify(exactly = 0) { publisher.markFailed(any()) }
    }

    @Test
    fun `fails an acceptance once the user declined signing it with our own account`() {
        coEvery { chatAcceptanceRepository.get(contactAccountId) } returns acceptance
        coEvery { accountResolver.resolveAcceptance(contact, "request") } returns
            Result.failure(IdentitySigningDeclined(contactAccountId))
        coEvery { publisher.markFailed(acceptance) } returns Result.success(Unit)

        startDelivery()

        coVerify(exactly = 1) { publisher.markFailed(acceptance) }
        coVerify(exactly = 0) { publisher.publishFirstDelivery(any(), any(), any()) }
    }

    private fun startDelivery() {
        with(UnconfinedComputationalScope()) { service.initialize() }
    }

    private class UnconfinedComputationalScope : ComputationalScope, CoroutineScope by CoroutineScope(Dispatchers.Unconfined)
}
