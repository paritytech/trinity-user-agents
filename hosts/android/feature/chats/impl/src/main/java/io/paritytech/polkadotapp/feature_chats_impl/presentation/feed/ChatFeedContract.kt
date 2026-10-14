package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatHeaderRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.CustomChatAppearance
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.HighlightedMessage
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatDisplayUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatInputUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMessagesState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatToolbarAction
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow

interface ChatFeedContract {
    val chatDisplay: StateFlow<LoadingState<ChatDisplayUiModel>>

    val config: StateFlow<ChatConfig>

    val popupState: StateFlow<MessagePopUpUiState?>

    val chatInputUiState: StateFlow<ChatInputUiState>

    val chatMessagesState: StateFlow<LoadingState<ChatMessagesState>>
    val menuState: StateFlow<ChatMenuState>
    val footerRenderer: StateFlow<CustomChatFooterRenderer?>
    val headerRenderer: StateFlow<CustomChatHeaderRenderer?>
    val menuRenderer: StateFlow<CustomChatMenuRenderer?>
    val customAppearance: StateFlow<CustomChatAppearance?>
    val revealingMessageId: StateFlow<ChatMessageId?>
    val scrollToPosition: SharedFlow<Int>
    val toolbarActions: StateFlow<ImmutableList<ChatToolbarAction>>

    val highlightEvents: Flow<HighlightedMessage>

    fun onBackClick()
    fun onMessageChange(message: String)
    fun onSendMessageClick()
    fun onPayClick()
    fun setChatActive()
    fun setChatInactive()

    fun onClearReply()
    fun onUnreadMessageVisible(message: ChatMessageUiModel)
    fun onMessageRevealComplete(messageId: ChatMessageId)
    fun onMenuClick()
    fun onCopyUsernameClick()
    fun onLeaveChatRequest()
    fun onLeaveChatConfirm()
    fun onBlockUserRequest()
    fun onBlockUserConfirm()
    fun onUnblockUserClick()
    fun onStartCallClick(withVideo: Boolean)
    fun onCloseMenu()

    fun onClearEdit()

    fun onMessageAction(action: MessageAction)

    fun onAcceptChatRequest()
    fun onDeclineChatRequest()

    fun onAttachClick()
}
