package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResource
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ResolvedChatImage
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatListRoomPolicyMapperTest {
    private val resolver = mockk<ChatResourceResolver>()
    private val mapper = ChatListRoomPolicyMapper(resolver)

    private val image = ChatResource.Preimage(ByteArray(32) { 9 }.toDataByteArray())
    private val hostIcon = "https://example.com/weather.png"

    @Test
    fun `a room without a product text shows the last message`() {
        assertNull(mapper.productPreview(chat(ChatRoomPolicy.Defaults)))
        assertNull(mapper.productPreview(chat(policy = null)))
        assertFalse(mapper.isOngoing(chat(ChatRoomPolicy.Defaults)))
    }

    @Test
    fun `the product text is the preview at the product timestamp`() {
        val policy = ChatRoomPolicy.Defaults.copy(
            listPreview = ChatRoomPolicy.ListPreview.Text("Bot is typing", ongoing = true),
            listOrder = ChatRoomPolicy.ListOrder(isPinned = false, timestamp = 42L),
        )

        val preview = mapper.productPreview(chat(policy, timestamp = 7L))

        assertEquals(LastMessageUiModel.Text(timestamp = 42L, isIncoming = true, message = "Bot is typing"), preview)
        assertTrue(mapper.isOngoing(chat(policy)))
    }

    @Test
    fun `without a product timestamp the preview has the time of the chat`() {
        val policy = ChatRoomPolicy.Defaults.copy(listPreview = ChatRoomPolicy.ListPreview.Text("Sunny", ongoing = false))

        assertEquals(7L, mapper.productPreview(chat(policy, timestamp = 7L))?.timestamp)
        assertFalse(mapper.isOngoing(chat(policy)))
    }

    @Test
    fun `an image badge resolves through the resource resolver`() = runBlocking<Unit> {
        coEvery { resolver.resolve(image) } returns Result.success(ResolvedChatImage.Remote("https://ipfs.example/badge"))

        assertEquals(
            ChatListUiState.Badge.Image(ResolvedChatImage.Remote("https://ipfs.example/badge")),
            mapper.badge(ChatSummaryBadge.Image(image)),
        )
    }

    @Test
    fun `an image badge that does not resolve shows no badge`() = runBlocking<Unit> {
        coEvery { resolver.resolve(image) } returns Result.failure(IllegalStateException("offline"))

        assertEquals(ChatListUiState.Badge.None, mapper.badge(ChatSummaryBadge.Image(image)))
    }

    @Test
    fun `counts map to number badges`() = runBlocking<Unit> {
        assertEquals(ChatListUiState.Badge.Unread(3), mapper.badge(ChatSummaryBadge.Unread(3)))
        assertEquals(ChatListUiState.Badge.None, mapper.badge(ChatSummaryBadge.None))
    }

    @Test
    fun `the product avatar wins over the icon of the room`() = runBlocking<Unit> {
        withHostIcon()
        coEvery { resolver.resolve(image) } returns Result.success(ResolvedChatImage.Inline(byteArrayOf(1, 2)))

        val avatar = mapper.avatar(chat(ChatRoomPolicy.Defaults.copy(avatar = image)))

        assertEquals(AvatarUiModel.Inline(byteArrayOf(1, 2)), avatar)
    }

    @Test
    fun `a product avatar that does not resolve falls back to the icon of the room`() = runBlocking<Unit> {
        withHostIcon()
        coEvery { resolver.resolve(image) } returns Result.failure(IllegalStateException("offline"))

        assertEquals(AvatarUiModel.Image(hostIcon), mapper.avatar(chat(ChatRoomPolicy.Defaults.copy(avatar = image))))
    }

    private fun withHostIcon() {
        every { resolver.resolveIcon(hostIcon) } returns Result.success(ResolvedChatImage.Remote(hostIcon))
    }

    private fun chat(policy: ChatRoomPolicy?, timestamp: Long = 0L) = Chat(
        id = ChatId.forExtensionRoom("ProductBot_weather", "berlin"),
        display = ChatDisplay("Weather", ChatAvatar.Url(hostIcon)),
        preview = ChatPreview.EmptyChat,
        timestamp = timestamp,
        unreadBadge = ChatSummaryBadge.None,
        hasUnseenReaction = false,
        customPreviewRenderer = null,
        roomPolicy = policy,
    )
}
