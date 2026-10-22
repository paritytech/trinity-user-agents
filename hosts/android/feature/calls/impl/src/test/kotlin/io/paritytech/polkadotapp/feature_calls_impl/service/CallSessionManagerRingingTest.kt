@file:OptIn(ExperimentalCoroutinesApi::class)

package io.paritytech.polkadotapp.feature_calls_impl.service

import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_calls_impl.state.CallStateHolder
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.minutes

class CallSessionManagerRingingTest {
    private val chatId = ChatId.forExtensionRoom("chat.dot", "dm-bob")
    private val roomCall = MutableStateFlow<ChatRoomCallModel?>(null)
    private val callStateHolder = mockk<CallStateHolder>(relaxed = true)
    private val roomCalls = object : ChatRoomCallRepository {
        override fun observe(chatId: ChatId) = roomCall
    }
    private val manager = CallSessionManager(
        externalCallSignaling = mockk(relaxed = true),
        callStateHolder = callStateHolder,
        callAudioRouteController = mockk(relaxed = true),
        peerChannelFactory = mockk(relaxed = true),
        productCallMedia = mockk(relaxed = true),
        roomCalls = roomCalls,
        roomEvents = mockk(relaxed = true),
    )

    @Test
    fun `a ringing product call rings until the product clears it, as a native call rings until its caller ends it`() = runTest {
        var canceled = false
        roomCall.value = ChatRoomCallModel.Ringing(callId = "c1", video = false, caller = "contact")

        with(backgroundScope) { manager.observeIncomingCallCanceled(chatId, "c1") { canceled = true } }
        advanceTimeBy(10.minutes)
        runCurrent()

        assertFalse(canceled)

        roomCall.value = null
        runCurrent()

        assertTrue(canceled)
        verify { callStateHolder.updateStatus(CallStatus.Ended) }
    }

    @Test
    fun `a pushed call that its product does not take up stops ringing`() = runTest {
        var canceled = false

        with(backgroundScope) { manager.observeIncomingCallCanceled(chatId, "c1") { canceled = true } }
        advanceTimeBy(1.minutes)
        runCurrent()

        assertTrue(canceled)
    }
}
