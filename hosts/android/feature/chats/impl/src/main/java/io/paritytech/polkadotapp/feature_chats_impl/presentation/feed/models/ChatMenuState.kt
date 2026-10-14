package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageRevisionUiModel
import kotlinx.collections.immutable.ImmutableList

sealed interface ChatMenuType {
    data class MainMenu(val actions: ImmutableList<ChatMenuAction>) : ChatMenuType
    data class LeaveConfirmation(val username: String) : ChatMenuType
    data class BlockConfirmation(val username: String) : ChatMenuType
    data class MessageHistory(val current: MessageRevisionUiModel, val history: ImmutableList<MessageRevisionUiModel>) : ChatMenuType
    data class Custom(val renderer: CustomChatMenuRenderer) : ChatMenuType
}

enum class ChatMenuAction {
    COPY_USERNAME,
    LEAVE_CHAT,
    BLOCK_USER
}

enum class ChatToolbarAction {
    AUDIO_CALL,
    VIDEO_CALL,
    MENU
}

@Immutable
data class ChatMenuState(
    val isVisible: Boolean = false,
    val type: ChatMenuType? = null
)
