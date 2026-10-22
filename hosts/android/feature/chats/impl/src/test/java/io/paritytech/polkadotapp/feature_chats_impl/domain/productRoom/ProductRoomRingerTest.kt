@file:OptIn(ExperimentalCoroutinesApi::class)

package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Test

class ProductRoomRingerTest {
    private val chatId = ChatId.forExtensionRoom("chat.dot", "dm-bob")
    private val calls = RealChatRoomCallRepository()
    private val callController = mockk<CallController>(relaxed = true)
    private val ringer = ProductRoomRinger(calls, callController, chatEngine = mockk())

    @Test
    fun `a call rings once, from its push or from its room, and needs no stored native message`() = runTest {
        with(TestComputationalScope(backgroundScope)) { ringer.initialize() }

        ringer.ringPushedCall(chatId, "c1", "bob", video = false)
        ringer.ringPushedCall(chatId, "c1", "bob", video = false)
        calls.set(chatId, ChatRoomCallModel.Ringing(callId = "c1", video = false, caller = "contact"))
        runCurrent()
        ringer.ringPushedCall(chatId, "c2", "bob", video = true)

        coVerify(exactly = 1) { callController.initiateIncomingCall(chatId, "c1", "bob", false) }
        coVerify(exactly = 1) { callController.initiateIncomingCall(chatId, "c2", "bob", true) }
    }

    private class TestComputationalScope(scope: CoroutineScope) : ComputationalScope, CoroutineScope by scope
}
