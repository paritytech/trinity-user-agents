package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject
import javax.inject.Singleton

interface ChatActiveTrackerInternal : ChatActiveTracker {
    fun setActive(chatId: ChatId)
    fun clear()
}

@Singleton
internal class RealChatActiveTracker @Inject constructor() : ChatActiveTracker, ChatActiveTrackerInternal {
    private val active = MutableStateFlow<ChatId?>(null)

    override fun subscribe(): Flow<ChatId?> = active

    override fun getActive(): ChatId? = active.value

    override fun setActive(chatId: ChatId) {
        active.value = chatId
    }

    override fun clear() {
        active.value = null
    }
}
