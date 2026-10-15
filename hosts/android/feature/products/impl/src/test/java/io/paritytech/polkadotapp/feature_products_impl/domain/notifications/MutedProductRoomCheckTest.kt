@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class MutedProductRoomCheckTest {
    private val product = ProductId.fromStoredValue("chat.dot")
    private val now = 10_000L

    @Test
    fun `a room link of the product names its room, decoded`() {
        assertEquals("main", productRoomOfLink(product, "polkadot://chat.dot/-/chat/main"))
        assertEquals("a b", productRoomOfLink(product, "polkadot://chat.dot/-/chat/a%20b?x=1"))
        assertEquals("main", productRoomOfLink(product, "https://chat.dot/-/chat/main"))
    }

    @Test
    fun `a link to another product, to another page or that does not parse names no room`() {
        assertNull(productRoomOfLink(product, "polkadot://other.dot/-/chat/main"))
        assertNull(productRoomOfLink(product, "polkadot://chat.dot/-/pocket"))
        assertNull(productRoomOfLink(product, "https://example.com/-/chat/main"))
        assertNull(productRoomOfLink(product, "not a link at all"))
    }

    @Test
    fun `a notification into a muted room is muted until the mute ends`() = runBlocking<Unit> {
        val main = ProductChatIdParameter("main").toChatId(product.toChatExtensionId())
        val states = mutableMapOf(main to ChatRoomUserStateModel(mutedUntil = now + 1, archived = false))
        val check = check(states)

        assertTrue(check.isMuted(product, "polkadot://chat.dot/-/chat/main"))
        assertFalse(check.isMuted(product, "polkadot://chat.dot/-/chat/other"))
        assertFalse(check.isMuted(product, null))

        states[main] = ChatRoomUserStateModel(mutedUntil = now, archived = true)
        assertFalse(check.isMuted(product, "polkadot://chat.dot/-/chat/main"))
    }

    private fun check(states: Map<ChatId, ChatRoomUserStateModel>) = MutedProductRoomCheck(
        userStateRepository = object : ChatRoomUserStateRepository {
            override fun observe(chatId: ChatId): Flow<ChatRoomUserStateModel> = flowOf(states[chatId] ?: ChatRoomUserStateModel.NONE)

            override fun observeAll(): Flow<Map<ChatId, ChatRoomUserStateModel>> = flowOf(states)
        },
        timeProvider = object : TimeProvider {
            override fun now(): Instant = Instant.fromEpochMilliseconds(now)
        },
    )
}
