package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.AccountId

// TODO ideally we want to not hard-code knowledge about contact input states into chat module
// One way to implement that would be to introduce customizable user input zone, similar to footer and headers
// However this need some additional thinking, so we do that like that for simplicity
sealed class ChatUserInputState {
    data class SendMessage(
        val paymentState: Payment,
        val attachmentsSupported: Boolean
    ) : ChatUserInputState() {
        sealed interface Payment {
            data object NotAvailable : Payment
            data class Available(val paymentAddress: AccountId) : Payment
        }
    }

    data object AcceptChatRequest : ChatUserInputState()

    data object SendChatRequest : ChatUserInputState()

    data object WaitChatRequestApproval : ChatUserInputState()

    data object UserDeclinedChatRequest : ChatUserInputState()

    data object PeerLeft : ChatUserInputState()

    data object UnblockUser : ChatUserInputState()

    data object Nothing : ChatUserInputState()
}

fun ChatUserInputState.canSendMessage(): Boolean {
    return this is ChatUserInputState.SendMessage
}

fun ChatUserInputState.isPeerLeft(): Boolean {
    return this is ChatUserInputState.PeerLeft
}
