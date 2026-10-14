package io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageOriginDisplay

interface MessageOriginDisplayResolver {
    fun displayOf(messageOrigin: ChatMessageOrigin): MessageOriginDisplay
}
