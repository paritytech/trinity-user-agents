package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState

/** The input of a room of the chat product is the input of the native person chat in the state of its contact. */
object ProductContactInput {
    /** Null for a room without a contact. */
    fun of(policy: ChatRoomPolicy?): ChatUserInputState? {
        val contact = policy?.contact ?: return null

        return when (contact.state) {
            ChatContactStateModel.INVITE -> ChatUserInputState.SendChatRequest
            ChatContactStateModel.OUTGOING -> ChatUserInputState.WaitChatRequestApproval
            ChatContactStateModel.INCOMING -> ChatUserInputState.AcceptChatRequest
            ChatContactStateModel.BLOCKED -> ChatUserInputState.UnblockUser
            ChatContactStateModel.PEER_LEFT -> ChatUserInputState.PeerLeft
            // The native rule: a contact without an origin that forbids payments can be paid. Attachments go to HOP
            // nodes, so the room offers them only when the product declares HOP attachments.
            ChatContactStateModel.ESTABLISHED -> ChatUserInputState.SendMessage(
                paymentState = ChatUserInputState.SendMessage.Payment.Available(contact.account),
                attachmentsSupported = policy.features.hopAttachments,
            )
        }
    }
}
