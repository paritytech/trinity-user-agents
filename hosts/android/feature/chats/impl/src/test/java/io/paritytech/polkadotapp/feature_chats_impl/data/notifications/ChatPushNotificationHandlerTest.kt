package io.paritytech.polkadotapp.feature_chats_impl.data.notifications

import android.content.Context
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.ChatPushContent
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.DecodedChatPush
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder.Companion.MESSAGE_KEY
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder.Companion.PUSH_ID_KEY
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.StrippedChatPushContent
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toEncodedMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ProcessedChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessagePlacement
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveConflictStrategy
import io.paritytech.polkadotapp.feature_chats_impl.utils.ChatMessageMappingHelper
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementRequestDecoder
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

class ChatPushNotificationHandlerTest {
    private val incomingChatPushDecoder: IncomingChatPushDecoder = mockk()
    private val chatNotificationPublisher: ChatNotificationPublisher = mockk(relaxed = true)
    private val tokenAmountFormatter: TokenAmountFormatter = mockk(relaxed = true)
    private val messageMappingHelper: ChatMessageMappingHelper = mockk(relaxed = true)
    private val chatActiveTracker: ChatActiveTracker = mockk()
    private val appLifecycleObserver: AppLifecycleObserver = mockk()
    private val chatEngine: ChatEngine = mockk()
    private val callController: CallController = mockk(relaxed = true)
    private val processedChatMessageRepository: ProcessedChatMessageRepository = mockk()
    private val appContext: Context = mockk(relaxed = true)
    private val chatMessageRepository: ChatMessageRepository = mockk()
    private val statementRequestDecoder: StatementRequestDecoder = mockk()
    private val contactsRepository: ContactsRepository = mockk()
    private val accountRepository: AccountRepository = mockk()
    private val chainRegistry: ChainRegistry = mockk()
    private val knownChains: KnownChains = mockk()

    private val handler = ChatPushNotificationHandler(
        incomingChatPushDecoder = incomingChatPushDecoder,
        chatMessageRepository = chatMessageRepository,
        chatNotificationPublisher = chatNotificationPublisher,
        tokenAmountFormatter = tokenAmountFormatter,
        messageMappingHelper = messageMappingHelper,
        statementRequestDecoder = statementRequestDecoder,
        chatActiveTracker = chatActiveTracker,
        appLifecycleObserver = appLifecycleObserver,
        chatEngine = chatEngine,
        fallbackUsernameGenerator = mockk(),
        callController = callController,
        processedChatMessageRepository = processedChatMessageRepository,
        contactsRepository = contactsRepository,
        accountRepository = accountRepository,
        chainRegistry = chainRegistry,
        knownChains = knownChains,
        compactionExpansionStarter = mockk(),
        appContext = appContext,
    )

    private val contactAccountId = ByteArray(32) { 1 }.intoAccountId()
    private val chatId = ChatId.fromContact(contactAccountId)

    @Before
    fun setUp() {
        every { appLifecycleObserver.getCurrentState() } returns AppLifecycleState.BACKGROUND
        every { chatActiveTracker.getActive() } returns null
        coEvery { chatEngine.saveMessage(any(), any(), any(), any()) } returns true
    }

    @Test
    fun `full message is saved and notified`() = runBlocking<Unit> {
        withDecodedPush(ChatPushContent.Full(message(ChatMessage.Content.Text("hi"))))

        handler.handle(LEGACY_PUSH)

        verifyMessageSaved()
        verifyNotificationPublished("hi")
    }

    @Test
    fun `stripped message is notified but never saved`() = runBlocking<Unit> {
        withDecodedPush(stripped(StrippedChatPushContent.Regular(ChatMessage.Content.Text("hi"))))

        handler.handle(LEGACY_PUSH)

        verifyNothingSaved()
        verifyNotificationPublished("hi")
    }

    @Test
    fun `undecodable message falls back to an unsupported notification without saving`() = runBlocking<Unit> {
        withUnsupportedText()
        withDecodedPush(stripped(StrippedChatPushContent.Regular(ChatMessage.Content.Unsupported(byteArrayOf(1)))))

        handler.handle(LEGACY_PUSH)

        verifyNothingSaved()
        verifyNotificationPublished(UNSUPPORTED_TEXT)
    }

    @Test
    fun `stripped payment is notified with its amount`() = runBlocking<Unit> {
        withPaymentText()
        withDecodedPush(stripped(StrippedChatPushContent.CoinagePayment(5_000_000L.intoBalance())))

        handler.handle(LEGACY_PUSH)

        verifyNothingSaved()
        verifyNotificationPublished(PAYMENT_TEXT)
    }

    @Test
    fun `stripped call offer rings without saving`() = runBlocking<Unit> {
        withOfferNotYetProcessed()
        val offer = ChatMessage.Content.DataChannelOffer(byteArrayOf(1), ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL)
        withDecodedPush(stripped(StrippedChatPushContent.Regular(offer)))

        handler.handle(LEGACY_PUSH)

        verifyNothingSaved()
        coVerify { callController.initiateIncomingCall(chatId, MESSAGE_ID, CALLER_NAME, true) }
    }

    @Test
    fun `statement messages are saved oldest first`() = runBlocking<Unit> {
        withStatementMessages(textMessage(NEWER_ID, timestamp = 2_000L), textMessage(OLDER_ID, timestamp = 1_000L))

        handler.handle(NEW_SPEC_PUSH)

        coVerifyOrder {
            chatEngine.saveMessage(match { it.id == OLDER_ID }, ChatMessageSaveConflictStrategy.IGNORE, ChatMessagePlacement.Latest, any())
            chatEngine.saveMessage(match { it.id == NEWER_ID }, ChatMessageSaveConflictStrategy.IGNORE, ChatMessagePlacement.Latest, any())
        }
    }

    private fun withStatementMessages(vararg messages: ChatMessage) {
        val contact = mockk<Contact>(relaxed = true) {
            every { username } returns CALLER_NAME
            every { accountId } returns contactAccountId
            every { isBlocked } returns false
            every { ourMetaAccountId } returns OUR_META_ACCOUNT_ID
        }
        val ownAccount = mockk<MetaAccount> { every { accountIdIn(any()) } returns contactAccountId }
        coEvery { contactsRepository.getContact(any()) } returns contact
        coEvery { accountRepository.getAccountById(OUR_META_ACCOUNT_ID) } returns ownAccount
        every { knownChains.people } returns PEOPLE_CHAIN_ID
        coEvery { chainRegistry.getChain(PEOPLE_CHAIN_ID) } returns mockk()
        coEvery { statementRequestDecoder.decodeMessages(any(), any(), any(), any()) } returns
            Result.success(messages.map { it.toEncodedMessage().getOrThrow() })
        coEvery { chatMessageRepository.getMessageStatuses(any()) } returns emptyMap()
    }

    private fun textMessage(id: String, timestamp: Long) = ChatMessage(
        id = id,
        chatId = chatId,
        timestamp = timestamp,
        origin = ChatMessageOrigin.Contact(contactAccountId),
        content = ChatMessage.Content.Text(id),
        status = ChatMessage.Status.NEW,
    )

    private fun withDecodedPush(content: ChatPushContent) {
        val contact = mockk<Contact> {
            every { username } returns CALLER_NAME
            every { accountId } returns contactAccountId
        }
        coEvery { incomingChatPushDecoder.decode(any()) } returns Result.success(DecodedChatPush(contact, chatId, content))
    }

    private fun withPaymentText() {
        every { appContext.getString(RCommon.string.chat_notification_payment, *anyVararg()) } returns PAYMENT_TEXT
    }

    private fun withUnsupportedText() {
        every { appContext.getString(RCommon.string.chat_message_unsupported) } returns UNSUPPORTED_TEXT
    }

    private fun withOfferNotYetProcessed() {
        coEvery { processedChatMessageRepository.tryMarkProcessed(MESSAGE_ID) } returns true
    }

    private fun verifyMessageSaved() {
        coVerify(exactly = 1) { chatEngine.saveMessage(match { it.id == MESSAGE_ID }, any(), any(), any()) }
    }

    private fun verifyNothingSaved() {
        coVerify(exactly = 0) { chatEngine.saveMessage(any(), any(), any(), any()) }
    }

    private fun verifyNotificationPublished(text: String) {
        verify { chatNotificationPublisher.publishNewMessageReceived(chatId, CALLER_NAME, text) }
    }

    private fun stripped(content: StrippedChatPushContent) = ChatPushContent.Stripped(
        messageId = MESSAGE_ID,
        timestamp = 1L,
        content = content,
    )

    private fun message(content: ChatMessage.Content) = ChatMessage(
        id = MESSAGE_ID,
        chatId = chatId,
        timestamp = 1L,
        origin = ChatMessageOrigin.Contact(contactAccountId),
        content = content,
        status = ChatMessage.Status.NEW,
    )

    private companion object {
        const val MESSAGE_ID = "message-id"
        const val CALLER_NAME = "alice"
        const val PAYMENT_TEXT = "is sending you $5"
        const val UNSUPPORTED_TEXT = "unsupported"

        const val OLDER_ID = "older"
        const val NEWER_ID = "newer"
        const val OUR_META_ACCOUNT_ID = 1L
        const val PEOPLE_CHAIN_ID = "people"

        val LEGACY_PUSH = mapOf(PUSH_ID_KEY to "00", MESSAGE_KEY to "00")
        val NEW_SPEC_PUSH = mapOf(
            "sender_pubkey" to "01".repeat(32),
            "statement_topic" to "00",
            "statement_data" to "00",
        )
    }
}
