package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** A product notification into a room shows as a person chat notification (Chat Extension v2 "Room notifications"). */
class ProductRoomNotificationPolicyTest {
    private val product = ProductId.fromStoredValue("chat.dot")
    private val bob = ProductChatIdParameter("dm-7").toChatId(product.toChatExtensionId())
    private val link = "polkadot://chat.dot/-/chat/dm-7"

    private var appState = AppLifecycleState.FOREGROUND
    private var active: ChatId? = null

    private val policy = ProductRoomNotificationPolicy(
        rooms = mockk<ReadOnlyChatRoomRepository> {
            // MockK hands a value class argument over as its boxed or its inner value.
            coEvery { getRoomMetadata(any()) } answers {
                val named = args[0] == bob || args[0] == bob.value
                RoomMetadata(name = "bob.42".takeIf { named }, icon = null)
            }
        },
        chatActiveTracker = mockk<ChatActiveTracker> { every { getActive() } answers { active } },
        appLifecycleObserver = mockk<AppLifecycleObserver> { every { getCurrentState() } answers { appState } },
    )

    @Test
    fun `a notification into a room has the room name as title`() = runBlocking<Unit> {
        assertEquals(ProductNotificationPresentation.Shown("bob.42"), policy.presentation(product, link))
        assertEquals(
            ProductNotificationPresentation.Shown(null),
            policy.presentation(product, "polkadot://chat.dot/-/chat/unknown"),
        )
    }

    @Test
    fun `the room on screen shows nothing, another screen or the background shows the notification`() = runBlocking<Unit> {
        active = bob
        assertEquals(ProductNotificationPresentation.Hidden, policy.presentation(product, link))

        active = ProductChatIdParameter("dm-8").toChatId(product.toChatExtensionId())
        assertEquals(ProductNotificationPresentation.Shown("bob.42"), policy.presentation(product, link))

        active = bob
        appState = AppLifecycleState.BACKGROUND
        assertEquals(ProductNotificationPresentation.Shown("bob.42"), policy.presentation(product, link))
    }

    @Test
    fun `a notification with no room of the product keeps the default title and always shows`() = runBlocking<Unit> {
        active = bob
        assertEquals(ProductNotificationPresentation.Shown(null), policy.presentation(product, null))
        assertEquals(
            ProductNotificationPresentation.Shown(null),
            policy.presentation(product, "polkadot://other.dot/-/chat/dm-7"),
        )
    }
}
