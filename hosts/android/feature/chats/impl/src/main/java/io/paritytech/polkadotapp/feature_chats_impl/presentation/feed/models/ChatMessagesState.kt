package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.FirstNewMessageInfo
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

@Immutable
data class ChatMessagesState(
    val messages: ImmutableList<ChatMessageUiModel> = persistentListOf(),
    val firstNewMessageInfo: FirstNewMessageInfo? = null,
    val unreadCounter: Int = 0
)
