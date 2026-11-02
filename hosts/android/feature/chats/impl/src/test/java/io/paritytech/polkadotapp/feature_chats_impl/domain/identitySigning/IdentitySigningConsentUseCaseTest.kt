package io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.IdentitySigningConsentRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.ContactChatSessionRestarter
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer.ContactChatSigners
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import kotlin.reflect.KClass

class IdentitySigningConsentUseCaseTest {
    private val contactAccountId = ByteArray(32) { 7 }.toDataByteArray()
    private val chatId = ChatId.fromContact(contactAccountId)

    private val consentRepository: IdentitySigningConsentRepository = mockk(relaxUnitFun = true)
    private val contactsRepository: ContactsRepository = mockk()
    private val chatMessageRepository: ChatMessageRepository = mockk(relaxUnitFun = true)
    private val chatSigners: ContactChatSigners = mockk()
    private val sessionRestarter: ContactChatSessionRestarter = mockk(relaxUnitFun = true)

    private val useCase = IdentitySigningConsentUseCase(
        consentRepository, contactsRepository, chatMessageRepository, chatSigners, sessionRestarter,
    )

    @Before
    fun setUp() {
        val transaction = slot<suspend () -> Unit>()
        coEvery { contactsRepository.withTransaction<Unit>(capture(transaction)) } coAnswers { transaction.captured() }
        coEvery { chatSigners.release(contactAccountId) } returns Result.success(Unit)
    }

    @Test
    fun `declining fails the waiting messages and drops the session queue`() = runBlocking<Unit> {
        useCase.decline(contactAccountId).getOrThrow()

        coVerify { consentRepository.set(contactAccountId, IdentitySigningConsent.DECLINED) }
        listOf(ChatMessage.Status.NEW, ChatMessage.Status.PROCESSING).forEach { status ->
            coVerify {
                chatMessageRepository.updateOutgoingMessagesStatusForChatExcludingTypes(chatId, status, ChatMessage.Status.DELIVERY_FAILED, any())
            }
        }
        coVerify { sessionRestarter.restartSession(contactAccountId) }
    }

    @Test
    fun `declining leaves the request and the acceptance to their own delivery`() = runBlocking<Unit> {
        val kept = slot<List<KClass<out ChatMessage.Content>>>()
        coEvery {
            chatMessageRepository.updateOutgoingMessagesStatusForChatExcludingTypes(chatId, any(), any(), capture(kept))
        } returns Unit

        useCase.decline(contactAccountId).getOrThrow()

        assertTrue(ChatMessage.Content.ChatRequest::class in kept.captured)
        assertTrue(ChatMessage.Content.DeviceChatAccepted::class in kept.captured)
    }

    @Test
    fun `revoking forgets the answer`() = runBlocking<Unit> {
        useCase.revoke(contactAccountId)

        coVerify(exactly = 1) { consentRepository.clear(contactAccountId) }
    }

    @Test
    fun `a new message after declining asks again`() = runBlocking<Unit> {
        coEvery { consentRepository.get(contactAccountId) } returns IdentitySigningConsent.DECLINED

        useCase.reopenIfDeclined(contactAccountId).getOrThrow()

        coVerify { consentRepository.clear(contactAccountId) }
    }

    @Test
    fun `a new message keeps a granted consent`() = runBlocking<Unit> {
        coEvery { consentRepository.get(contactAccountId) } returns IdentitySigningConsent.GRANTED

        useCase.reopenIfDeclined(contactAccountId).getOrThrow()

        coVerify(exactly = 0) { consentRepository.clear(any()) }
    }
}
