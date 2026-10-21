package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ProductNotificationPublisher
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatPushPreview
import uniffi.truapi.HostChatPushRoute
import uniffi.truapi.HostPushDelivery
import uniffi.truapi.HostPushService
import io.paritytech.polkadotapp.common.R as RCommon

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
    fun `the product handler takes only pushes of its routes, shows their text and wakes nothing without a runtime`() = runTest {
        val routes = routes().apply { replace("chat.paseo", listOf(HostChatPushRoute(pushId, "dm-1", "Alice", key))) }
        val publisher = mockk<ProductNotificationPublisher>(relaxed = true)
        val context = mockk<Context> { every { getString(RCommon.string.product_chat_push_video_call) } returns "Incoming video call" }
        var preview: ChatPushPreview? = ChatPushPreview.Message("m1", "hello")
        val handler = ProductChatPushHandler(
            routes = routes,
            decoder = { _, _ -> preview },
            publisher = publisher,
            runtimeProvider = { mockk { coEvery { runtime() } returns Result.failure(IllegalStateException("not booted")) } },
            context = context,
        )

        assertTrue(handler.canHandle(mapOf("pushId" to pushId, "message" to "00")))
        assertFalse(handler.canHandle(mapOf("pushId" to "ef".repeat(32), "message" to "00")))
        assertTrue(handler.priority > 0)

        handler.handle(mapOf("pushId" to pushId, "message" to "00"))
        preview = ChatPushPreview.Call("m2", video = true)
        handler.handle(mapOf("pushId" to pushId, "message" to "00"))
        preview = null
        handler.handle(mapOf("pushId" to pushId, "message" to "00"))

        val product = ProductId.fromStoredValue("chat.paseo")
        coVerify(exactly = 1) { publisher.publishProductNotification(product, any(), "hello", "polkadot://chat.paseo/-/chat/dm-1") }
        coVerify(exactly = 1) { publisher.publishProductNotification(product, any(), "Incoming video call", any()) }
        coVerify(exactly = 2) { publisher.publishProductNotification(any(), any(), any(), any()) }
    }
}
