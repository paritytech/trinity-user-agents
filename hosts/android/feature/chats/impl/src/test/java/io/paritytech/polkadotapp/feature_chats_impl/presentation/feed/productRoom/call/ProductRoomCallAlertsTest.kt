package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallNotifier
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatRoomEventPublisher
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class ProductRoomCallAlertsTest {
    private val room = ChatId.forExtensionRoom("ProductBot_sample.dot", "host")
    private val calls = RealChatRoomCallRepository()
    private val lifecycle = MutableStateFlow(AppLifecycleState.FOREGROUND)
    private val publisher = mockk<ChatRoomEventPublisher>(relaxed = true)
    private val notifier = mockk<ProductCallNotifier>(relaxed = true) {
        every { showRinging(any()) } returns true
    }
    private val intents = mockk<ProductRoomCallIntents>(relaxed = true) {
        every { tag(any()) } returns "tag-host"
    }
    private val rooms = mockk<ReadOnlyChatRoomRepository> {
        coEvery { getRoomMetadata(any()) } returns RoomMetadata(name = "Sample: host store", icon = null)
    }
    private val appLifecycle = mockk<AppLifecycleObserver> {
        every { subscribe() } returns lifecycle
    }

    private fun TestScope.alerts() = ProductRoomCallAlerts(calls, publisher, rooms, appLifecycle, notifier, intents).also {
        with(ComputationalScope(backgroundScope)) { it.initialize() }
        runCurrent()
    }

    private val ringing = ChatRoomCallModel.Ringing("c1", video = false, caller = "p1")

    @Test
    fun `a ringing call that nobody answers ends after 60 s`() = runTest {
        alerts()
        calls.set(room, ringing)
        runCurrent()

        advanceTimeBy(59.seconds)
        assertEquals(ringing, calls.observeAll().value[room])

        advanceTimeBy(2.seconds)
        assertNull(calls.observeAll().value[room])
    }

    @Test
    fun `an answered call is not cleared by the timer of its ringing`() = runTest {
        alerts()
        calls.set(room, ringing)
        runCurrent()
        val active = ChatRoomCallModel.Active("c1", video = false, startedAt = 1, mediaSession = null)
        calls.set(room, active)
        runCurrent()

        advanceTimeBy(120.seconds)

        assertEquals(active, calls.observeAll().value[room])
    }

    @Test
    fun `another screen of the app shows the in-app banner, and the room screen does not`() = runTest {
        val alerts = alerts()

        calls.set(room, ringing)
        runCurrent()
        assertEquals(ProductRoomCallBannerUiModel(room, "c1", "Sample: host store", video = false), alerts.bannerState.value)

        alerts.onRoomShown(room, shown = true)
        runCurrent()
        assertNull(alerts.bannerState.value)
        verify(exactly = 0) { notifier.showRinging(any()) }
    }

    @Test
    fun `an app in the background shows one notification for each ringing call and removes it when the call stops`() = runTest {
        val alerts = alerts()
        lifecycle.value = AppLifecycleState.BACKGROUND

        calls.set(room, ringing)
        runCurrent()
        calls.set(room, ringing.copy(caller = "p2"))
        runCurrent()
        verify(exactly = 1) { notifier.showRinging(match { it.tag == "tag-host" && it.roomName == "Sample: host store" }) }
        assertNull(alerts.bannerState.value)

        calls.set(room, null)
        runCurrent()
        verify { notifier.cancelRinging("tag-host") }
    }

    @Test
    fun `the notification goes when the app comes to the front, and the banner shows`() = runTest {
        val alerts = alerts()
        lifecycle.value = AppLifecycleState.BACKGROUND
        calls.set(room, ringing)
        runCurrent()

        lifecycle.value = AppLifecycleState.FOREGROUND
        runCurrent()

        verify { notifier.cancelRinging("tag-host") }
        assertEquals("c1", alerts.bannerState.value?.callId)
    }

    @Test
    fun `an answer from the notification reaches the product only while the call rings`() = runTest {
        val alerts = alerts()
        calls.set(room, ringing)
        runCurrent()

        alerts.answerFromNotification(room, "c1", accept = false)
        alerts.answerFromNotification(room, "other", accept = true)
        runCurrent()

        coVerify(exactly = 1) { publisher.publish(room, ChatRoomEventModel.CallAnswered("c1", accept = false)) }
        coVerify(exactly = 1) { publisher.publish(any(), any()) }
    }
}
