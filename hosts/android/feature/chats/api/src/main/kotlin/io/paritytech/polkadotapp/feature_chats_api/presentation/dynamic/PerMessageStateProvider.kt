package io.paritytech.polkadotapp.feature_chats_api.presentation.dynamic

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import kotlinx.coroutines.flow.Flow

interface PerMessageStateProvider<S> {
    fun observe(id: ChatMessageId): Flow<S?>
}
