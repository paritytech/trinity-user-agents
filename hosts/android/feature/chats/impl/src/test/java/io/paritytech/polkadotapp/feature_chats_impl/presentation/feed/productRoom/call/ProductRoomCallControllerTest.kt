package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_calls_api.domain.models.ActiveCallState
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatRoomEventPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatParticipantUiModel
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ProductRoomCallControllerTest {
    private val room = ChatId.forExtensionRoom("ProductBot_sample.dot", "host")
    private val policy = MutableStateFlow<ChatRoomPolicy?>(ChatRoomPolicy.Defaults)
    private val calls = RealChatRoomCallRepository()
    private val publisher = mockk<ChatRoomEventPublisher>(relaxed = true)
    private val alerts = mockk<ProductRoomCallAlerts>(relaxed = true)
    private val ongoingCalls = mockk<CallStateTracker> { every { getActiveCall() } returns null }
    private val callController = mockk<CallController>(relaxed = true)
    private val caller = ChatParticipantUiModel("p1", "Alice", "Guest", AvatarUiModel.Name("Alice", AvatarColorScheme.from("p1".encodeToByteArray())))

    private fun TestScope.controller(served: Collection<String> = listOf(ChatHostCapabilities.CALL_BUTTONS)) =
        ProductRoomCallController(
            scope = backgroundScope,
            chatId = room,
            roomPolicy = policy,
            calls = calls,
            publisher = publisher,
            alerts = alerts,
            ongoingCalls = ongoingCalls,
            callController = callController,
            resolveCaller = { id -> caller.takeIf { id == "p1" } },
            servedFeatures = served,
            onFailure = {},
        ).also { runCurrent() }

    private fun declares(audio: Boolean, video: Boolean) {
        policy.value = ChatRoomPolicy.Defaults.copy(
            features = ChatRoomFeaturesModel.HOST_DEFAULT.copy(calls = ChatRoomCallsModel(audio, video)),
        )
    }

    @Test
    fun `the header shows the declared call buttons, and a press asks the product for a call`() = runTest {
        val controller = controller()
        assertNull(controller.uiState.value.buttons)

        declares(audio = true, video = false)
        runCurrent()
        assertEquals(ProductRoomCallUiState.Buttons(audio = true, video = false), controller.uiState.value.buttons)

        controller.onAction(ProductRoomCallAction.Request(video = true))
        runCurrent()
        coVerify { publisher.publish(room, ChatRoomEventModel.CallRequested(video = true)) }
    }

    @Test
    fun `a host that does not list call_buttons shows no buttons`() = runTest {
        val controller = controller(served = emptyList())

        declares(audio = true, video = true)
        runCurrent()

        assertNull(controller.uiState.value.buttons)
    }

    @Test
    fun `a ringing call shows its caller, and Accept and Decline answer it`() = runTest {
        val controller = controller()

        calls.set(room, ChatRoomCallModel.Ringing("c1", video = true, caller = "p1"))
        runCurrent()
        val ringing = controller.uiState.value.call as ProductRoomCallUiState.Call.Ringing
        assertEquals("Alice", ringing.callerName)
        assertTrue(ringing.video)

        controller.onAction(ProductRoomCallAction.Answer(accept = false))
        controller.onAction(ProductRoomCallAction.Answer(accept = true))
        runCurrent()

        coVerify { publisher.publish(room, ChatRoomEventModel.CallAnswered("c1", accept = false)) }
        coVerify { publisher.publish(room, ChatRoomEventModel.CallAnswered("c1", accept = true)) }
    }

    @Test
    fun `a caller that the room cannot name shows its id`() = runTest {
        val controller = controller()

        calls.set(room, ChatRoomCallModel.Ringing("c1", video = false, caller = "p9"))
        runCurrent()

        assertEquals("p9", (controller.uiState.value.call as ProductRoomCallUiState.Call.Ringing).callerName)
    }

    @Test
    fun `a call without media sends CallOpened on a tap and CallEnded on End`() = runTest {
        val controller = controller()
        calls.set(room, ChatRoomCallModel.Active("c2", video = false, startedAt = 1_000, mediaSession = null))
        runCurrent()

        controller.onAction(ProductRoomCallAction.Open)
        controller.onAction(ProductRoomCallAction.End)
        runCurrent()

        coVerify { publisher.publish(room, ChatRoomEventModel.CallOpened("c2")) }
        coVerify { publisher.publish(room, ChatRoomEventModel.CallEnded("c2")) }
        assertFalse(controller.uiState.value.showsCallScreen)
    }

    @Test
    fun `a call on the call engine of the host reopens its call screen on a tap`() = runTest {
        every { ongoingCalls.getActiveCall() } returns
            ActiveCallState(room, "c4", CallDirection.Outgoing, CallStatus.Ringing, initiatedWithVideo = false)
        val controller = controller()
        calls.set(room, ChatRoomCallModel.Active("c4", video = false, startedAt = 1_000, mediaSession = null))
        runCurrent()

        controller.onAction(ProductRoomCallAction.Open)
        runCurrent()

        verify { callController.openOngoingCallScreen() }
        coVerify(exactly = 0) { publisher.publish(room, ChatRoomEventModel.CallOpened("c4")) }
    }

    @Test
    fun `a call with a media session opens the call screen of the host, and End closes it`() = runTest {
        val controller = controller()
        calls.set(room, ChatRoomCallModel.Active("c3", video = true, startedAt = 1_000, mediaSession = DataByteArray(ByteArray(32))))
        runCurrent()

        controller.onAction(ProductRoomCallAction.Open)
        runCurrent()
        assertTrue(controller.uiState.value.showsCallScreen)
        coVerify(exactly = 0) { publisher.publish(room, ChatRoomEventModel.CallOpened("c3")) }

        controller.onAction(ProductRoomCallAction.End)
        runCurrent()
        assertFalse(controller.uiState.value.showsCallScreen)
        coVerify { publisher.publish(room, ChatRoomEventModel.CallEnded("c3")) }
    }

    @Test
    fun `the room screen stops the alerts of its room while it shows`() = runTest {
        val controller = controller()

        controller.onAction(ProductRoomCallAction.ScreenShown(shown = true))
        controller.onAction(ProductRoomCallAction.ScreenShown(shown = false))

        verify { alerts.onRoomShown(room, shown = true) }
        verify { alerts.onRoomShown(room, shown = false) }
    }

    @Test
    fun `the timer counts minutes and seconds, and hours from one hour`() {
        assertEquals("0:00", callElapsedText(startedAt = 5_000, now = 1_000))
        assertEquals("0:07", callElapsedText(startedAt = 0, now = 7_900))
        assertEquals("12:05", callElapsedText(startedAt = 0, now = 725_000))
        assertEquals("1:00:01", callElapsedText(startedAt = 0, now = 3_601_000))
    }
}
