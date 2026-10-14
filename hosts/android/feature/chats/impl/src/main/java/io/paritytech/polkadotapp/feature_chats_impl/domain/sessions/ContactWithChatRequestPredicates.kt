package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isPendingOutgoing

internal fun ContactWithChatRequest.shouldStartPolling(): Boolean {
    if (contact.isBlocked) return false
    return pendingChatRequest == null || pendingChatRequest?.isPendingOutgoing() == true
}
