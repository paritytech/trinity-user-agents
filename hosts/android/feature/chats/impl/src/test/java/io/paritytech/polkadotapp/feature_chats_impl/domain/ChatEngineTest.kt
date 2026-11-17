package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.DefaultRoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Instant

class ChatEngineTest {
    private val contactsRepository: ContactsRepository = mockk()
    private val chatDisplayGenerator: ChatDisplayGenerator = mockk()
    private val chatExtensionRegistry: ChatExtensionRegistry = mockk()

    private val chatEngine = ChatEngine(
        chatMessageRepository = mockk(),
        chatMessageProcessingRepository = mockk(),
        chatExtensionRegistry = chatExtensionRegistry,
        accountRepository = mockk(),
        contactsRepository = contactsRepository,
        messageRevisionRepository = mockk(),
        dmChatMessageOriginDisplayResolverFactory = mockk(),
        chatDisplayGenerator = chatDisplayGenerator,
        messageSaveProcessors = emptySet(),
        headerRenderers = emptyMap(),
        originConfigurations = emptyMap(),
        chatRoomRepository = mockk(),
        deleteRoomUseCase = mockk(),
        productRoomDeclarer = mockk(),
    )

    private val contactAccountId = ByteArray(32) { 1 }.intoAccountId()
    private val contact = Contact(
        accountId = contactAccountId,
        username = "alice",
        chatKey = ByteArray(32) { 2 }.requireX25519PublicKey(),
        ourMetaAccountId = 1,
        avatarUrl = null,
        sharedSecretDerivationDomain = SharedSecretDerivationDomain.CHAT,
        addedAt = Instant.fromEpochMilliseconds(0),
    )
    private val contactDisplay = ChatDisplay("alice", ChatAvatar.Account("alice", themeSeed = contactAccountId.value))

    @Test
    fun `contact removed while the chat is observed emits a failure instead of throwing`() = runTest {
        every { contactsRepository.subscribeContact(contactAccountId) } returns flowOf(contact, null)
        every { chatDisplayGenerator.generateAccountChatDisplay(contactAccountId, "alice", null) } returns contactDisplay

        val emissions = chatEngine.observeChatDisplay(ChatId.fromContact(contactAccountId)).toList()

        assertEquals(2, emissions.size)
        assertSame(contactDisplay, emissions[0].getOrNull())
        assertTrue(emissions[1].exceptionOrNull() is IllegalStateException)
    }

    // A product's icon loads on demand, so its chat shows the product's letter until the icon arrives.
    @Test
    fun `an extension icon is drawn over the extension's letter avatar`() = runTest {
        val chatId = ChatId.forExtensionRoom("dim2.paseo", "jollity")
        val extension = mockk<ChatExtension> {
            every { defaultRoomMetadata } returns DefaultRoomMetadata("Jollity", icon = "product-icon:dim2.paseo")
        }
        coEvery { chatExtensionRegistry.getExtensionForChat(chatId) } returns extension

        val avatar = chatEngine.getDefaultChatExtensionDisplay(chatId).getOrNull()?.avatar as? ChatAvatar.Url

        assertEquals("product-icon:dim2.paseo" to "Jollity", avatar?.url to avatar?.placeholder?.name)
    }
}
