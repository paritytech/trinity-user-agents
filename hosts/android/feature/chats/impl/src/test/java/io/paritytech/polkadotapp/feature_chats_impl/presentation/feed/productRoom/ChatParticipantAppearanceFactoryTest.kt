package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import android.content.Context
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageOriginDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ResolvedChatImage
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

class ChatParticipantAppearanceFactoryTest {
    private val context = mockk<Context> {
        every { getString(any()) } answers { "kind-${firstArg<Int>()}" }
        every { getString(RCommon.string.chat_participant_person) } returns "Person #%1${'$'}s"
        every { getString(RCommon.string.chat_participant_contact) } returns "Contact %1${'$'}s"
    }
    private val avatarBytes = byteArrayOf(7)
    private val resolver = mockk<ChatResourceResolver> {
        coEvery { resolve(any()) } returns Result.success(ResolvedChatImage.Inline(avatarBytes))
    }
    private val origins = object : MessageOriginDisplayResolver {
        override fun displayOf(messageOrigin: ChatMessageOrigin) = when (messageOrigin) {
            ChatMessageOrigin.User -> MessageOriginDisplay("You", ChatAvatar.Url("https://you"))
            else -> MessageOriginDisplay("Weather", ChatAvatar.Url("https://weather"))
        }
    }
    private val productOrigin = ChatMessageOrigin.Extension("ProductBot_weather")
    private val factory = ChatParticipantAppearanceFactory(context, resolver)

    @Test
    fun `the reserved ids draw the user and the product of the room`() = runBlocking {
        val user = factory.appearance("user", entry = null, origins, productOrigin)
        val product = factory.appearance("product", entry = null, origins, productOrigin)

        assertEquals("You", user.name)
        assertEquals(AvatarUiModel.Image("https://you"), user.avatar)
        assertEquals("Weather", product.name)
        assertEquals("kind-${RCommon.string.chat_participant_kind_product}", product.detail)
    }

    @Test
    fun `each kind draws its own name`() = runBlocking {
        val account = ByteArray(32) { it.toByte() }.toDataByteArray()

        assertEquals("Person #42", appearance(ChatParticipantModel.Person(42)).name)
        assertEquals("0x0001…1e1f", appearance(ChatParticipantModel.LitePerson(account)).name)
        assertEquals("Contact 0x0001…1e1f", appearance(ChatParticipantModel.Contact(account)).name)
        assertEquals("other.dot", appearance(ChatParticipantModel.Product("other.dot")).name)
    }

    @Test
    fun `a custom participant has its nickname and its avatar`() = runBlocking {
        val avatar = ChatResourceModel.Inline("image/png", avatarBytes.toDataByteArray())

        val custom = appearance(ChatParticipantModel.Custom("Ada", avatar))

        assertEquals("Ada", custom.name)
        assertEquals(AvatarUiModel.Inline(avatarBytes), custom.avatar)
    }

    @Test
    fun `an id that the product did not resolve shows the id`() = runBlocking {
        assertEquals("ghost", factory.appearance("ghost", entry = null, origins, productOrigin).name)
    }

    private suspend fun appearance(participant: ChatParticipantModel) =
        factory.appearance("p1", ChatParticipantEntryModel("p1", participant), origins, productOrigin)
}
