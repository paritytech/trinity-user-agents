package io.paritytech.polkadotapp.feature_chats_impl.domain.error

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId

class OutgoingChatRequestNotPending(requestId: ChatRequestId) : Throwable("Chat request $requestId is no longer pending")
