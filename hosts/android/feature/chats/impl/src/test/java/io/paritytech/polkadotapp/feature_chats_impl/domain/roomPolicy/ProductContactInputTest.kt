package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ProductContactInputTest {
    private val account = ByteArray(32) { 3 }.intoAccountId()

    @Test
    fun `a room without a contact has no person chat input`() {
        assertNull(ProductContactInput.of(null))
        assertNull(ProductContactInput.of(ChatRoomPolicy.Defaults))
    }

    @Test
    fun `each contact state shows the footer of the native person chat in that state`() {
        val expected = mapOf(
            ChatContactStateModel.INVITE to ChatUserInputState.SendChatRequest,
            ChatContactStateModel.OUTGOING to ChatUserInputState.WaitChatRequestApproval,
            ChatContactStateModel.INCOMING to ChatUserInputState.AcceptChatRequest,
            ChatContactStateModel.BLOCKED to ChatUserInputState.UnblockUser,
            ChatContactStateModel.PEER_LEFT to ChatUserInputState.PeerLeft,
        )

        for ((state, input) in expected) {
            assertEquals(state.name, input, ProductContactInput.of(policy(state)))
        }
    }

    @Test
    fun `an established contact gets the native composer, with attachments when the room declares HOP attachments`() {
        val withHop = ChatRoomFeaturesModel(calls = null, hopAttachments = true)

        assertEquals(
            ChatUserInputState.SendMessage(ChatUserInputState.SendMessage.Payment.Available(account), attachmentsSupported = true),
            ProductContactInput.of(policy(ChatContactStateModel.ESTABLISHED, withHop)),
        )
        assertEquals(
            ChatUserInputState.SendMessage(ChatUserInputState.SendMessage.Payment.Available(account), attachmentsSupported = false),
            ProductContactInput.of(policy(ChatContactStateModel.ESTABLISHED)),
        )
    }

    private fun policy(state: ChatContactStateModel, features: ChatRoomFeaturesModel? = null) = ChatRoomPolicy.from(
        declarations(features = features, contact = ChatRoomContactModel(account, username = "bob", state = state)),
    )
}
