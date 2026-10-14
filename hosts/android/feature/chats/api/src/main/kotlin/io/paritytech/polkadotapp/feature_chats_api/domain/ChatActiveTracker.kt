package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.flow.Flow

interface ChatActiveTracker {
    fun subscribe(): Flow<ChatId?>
    fun getActive(): ChatId?
}
