package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isPendingOutgoing

internal fun ContactWithChatRequest.shouldStartPolling(): Boolean {
    if (contact.isBlocked) return false
    return pendingChatRequest == null || pendingChatRequest?.isPendingOutgoing() == true
}

// chat-spec RFC-0005 leaves "active" to implementations: an established chat with a contact neither blocked nor gone.
internal fun Contact.isActiveChat(): Boolean {
    return pendingChatRequestId == null && !isBlocked && !isPeerLeft
}
