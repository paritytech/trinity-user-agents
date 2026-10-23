package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContact
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatNotifications
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatPushContent
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAttachmentType
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatPushContent
import uniffi.truapi.ChatPushMediaKind
import uniffi.truapi.ChatPushPreview
import uniffi.truapi.HostChatPushRoute
import uniffi.truapi.HostPushDelivery
import uniffi.truapi.HostPushService

class ProductChatPushTest {
    private val stored = mutableMapOf<String, String>()
    private val preferences = mockk<EncryptedPreferences> {
        every { putEncryptedString(any(), any()) } answers { stored[firstArg()] = secondArg() }
        every { getDecryptedString(any()) } answers { stored[firstArg()] }
    }
    private val pushId = "ab".repeat(32)
    private val key = ByteArray(32) { 7 }

    private fun routes() = ProductChatPushRoutes(preferences)

    @Test
    fun `chat push routes survive a restart and replace only their product's routes`() {
        routes().replace("chat.paseo", listOf(HostChatPushRoute(pushId, "dm-1", "Alice", key)))
        routes().replace("other.paseo", listOf(HostChatPushRoute("cd".repeat(32), "r", "Bob", key)))

        val reread = routes()
        val route = reread.find(pushId.uppercase())!!
        assertEquals("dm-1", route.roomId)
        assertEquals("Alice", route.title)
        assertArrayEquals(key, route.key)

        reread.replace("chat.paseo", emptyList())
        assertNull(routes().find(pushId))
        assertTrue(routes().find("cd".repeat(32)) != null)
    }

    @Test
    fun `only the chat product keeps push routes`() = runTest {
        val routes = routes()
        val bridge = AppPushHostBridge(mockk(), routes)

        bridge.setChatPushRoutes("game.paseo", listOf(HostChatPushRoute(pushId, "lobby", "Game", key)))
        assertNull(routes.find(pushId))

        bridge.setChatPushRoutes("chat.paseo", listOf(HostChatPushRoute(pushId, "dm-1", "Alice", key)))
        assertEquals("dm-1", routes.find(pushId)?.roomId)
    }

    @Test
    fun `a chat push goes to the relay as the native chat sends one`() = runTest {
        val helper = mockk<PushNotificationsHelper> {
            coEvery { sendNotify(any(), any(), any(), any(), any()) } returns Result.success(Unit)
        }
        val bridge = AppPushHostBridge(helper, routes())

        val taken = bridge.sendPush("chat.paseo", HostPushDelivery(HostPushService.APNS, "a1b2", pushId, "0102", voip = true))

        assertTrue(taken)
        coVerify {
            helper.sendNotify("a1b2", OperatingSystem.IOS, match { it.contentEquals(ByteArray(32) { 0xab.toByte() }) }, match { it.contentEquals(byteArrayOf(1, 2)) }, true)
        }
    }

    @Test
    fun `the product handler takes only pushes of its routes, words a message as the native chat and rings a call at once`() =
        runTest {
            val routes = routes().apply { replace("chat.paseo", listOf(HostChatPushRoute(pushId, "dm-1", "Alice", key))) }
            val notifications = mockk<PersonChatNotifications>(relaxed = true)
            var preview: ChatPushPreview? = ChatPushPreview.Message("m1", ChatPushContent.Text("hello"))
            val handler = handler(routes, notifications, contacts = emptyList()) { preview }

            assertTrue(handler.canHandle(mapOf("pushId" to pushId, "message" to "00")))
            assertFalse(handler.canHandle(mapOf("pushId" to "ef".repeat(32), "message" to "00")))
            assertTrue(handler.priority > 0)

            handler.handle(mapOf("pushId" to pushId, "message" to "00"))
            preview = ChatPushPreview.Call("m2", video = true)
            handler.handle(mapOf("pushId" to pushId, "message" to "00"))
            preview = ChatPushPreview.Message("m3", ChatPushContent.Edited("hey"))
            handler.handle(mapOf("pushId" to pushId, "message" to "00"))
            preview = null
            handler.handle(mapOf("pushId" to pushId, "message" to "00"))

            coVerify(exactly = 1) { notifications.showMessage(room, "Alice", PersonChatPushContent.Text("hello")) }
            coVerify(exactly = 1) { notifications.showMessage(any(), any(), any()) }
            coVerify(exactly = 1) { notifications.ringCall(room, "Alice", "m2", video = true) }
        }

    @Test
    fun `a push of a blocked contact shows nothing and a known contact goes by its username`() = runTest {
        val routes = routes().apply { replace("chat.paseo", listOf(HostChatPushRoute(pushId, "dm-1", "Alice", key))) }
        val notifications = mockk<PersonChatNotifications>(relaxed = true)
        val alice = PersonChatContact(DataByteArray(ByteArray(32) { 1 }), "alice.01", room, blocked = false)

        handler(routes, notifications, listOf(alice.copy(blocked = true))) { ChatPushPreview.Call("m1", video = false) }
            .handle(mapOf("pushId" to pushId, "message" to "00"))
        handler(routes, notifications, listOf(alice)) { ChatPushPreview.Message("m2", ChatPushContent.Reaction("👍")) }
            .handle(mapOf("pushId" to pushId, "message" to "00"))

        coVerify(exactly = 0) { notifications.ringCall(any(), any(), any(), any()) }
        coVerify(exactly = 1) { notifications.showMessage(room, "alice.01", PersonChatPushContent.Reaction("👍")) }
    }

    @Test
    fun `each kind of a pushed message is the kind the native chat words`() {
        assertEquals(
            PersonChatPushContent.RichText("look", MessageAttachmentType.IMAGE),
            ChatPushContent.RichText("look", listOf(ChatPushMediaKind.IMAGE, ChatPushMediaKind.VIDEO)).toPersonChatContent(),
        )
        assertEquals(PersonChatPushContent.RichText("yes", null), ChatPushContent.Reply("yes", emptyList()).toPersonChatContent())
        assertNull(ChatPushContent.Reply(null, emptyList()).toPersonChatContent())
        assertEquals(PersonChatPushContent.RequestAccepted, ChatPushContent.RequestAccepted.toPersonChatContent())
        assertEquals(PersonChatPushContent.ContactAdded, ChatPushContent.ContactAdded.toPersonChatContent())
        assertEquals(PersonChatPushContent.Unsupported, ChatPushContent.Payment("5", legacy = true).toPersonChatContent())
        assertTrue(ChatPushContent.Payment("5", legacy = false).toPersonChatContent() is PersonChatPushContent.Payment)
        assertNull(ChatPushContent.LeftChat.toPersonChatContent())
        assertNull(ChatPushContent.ReactionRemoved.toPersonChatContent())
    }

    private val room = ProductChatIdParameter("dm-1").toChatId(ProductId.fromStoredValue("chat.paseo").toChatExtensionId())

    @Test
    fun `a push with nothing to show, such as a hang-up, still wakes the product`() = runTest {
        val routes = routes().apply { replace("chat.paseo", listOf(HostChatPushRoute(pushId, "dm-1", "Alice", key))) }
        val notifications = mockk<PersonChatNotifications>(relaxed = true)
        val runtimeProvider = mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.failure(IllegalStateException("not booted")) }

        handler(routes, notifications, contacts = emptyList(), runtimeProvider = runtimeProvider) { null }
            .handle(mapOf("pushId" to pushId, "message" to "00"))

        coVerify(exactly = 1) { runtimeProvider.runtime() }
        coVerify(exactly = 0) { notifications.showMessage(any(), any(), any()) }
    }

    private fun handler(
        routes: ProductChatPushRoutes,
        notifications: PersonChatNotifications,
        contacts: List<PersonChatContact>,
        runtimeProvider: TrUAPIHostRuntimeProvider = mockk { coEvery { runtime() } returns Result.failure(IllegalStateException("not booted")) },
        preview: () -> ChatPushPreview?,
    ) = ProductChatPushHandler(
        routes = routes,
        decoder = { _, _ -> preview() },
        notifications = notifications,
        contacts = mockk<PersonChatContacts> { coEvery { getContacts() } returns contacts },
        fallbackUsernameGenerator = mockk<FallbackUsernameGenerator> { every { generateFromAccountId(any()) } returns "fallback" },
        runtimeProvider = { runtimeProvider },
    )
}
