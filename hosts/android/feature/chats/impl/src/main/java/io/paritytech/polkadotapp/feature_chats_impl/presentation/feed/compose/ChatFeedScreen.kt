package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.dataOrNull
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.Mock
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.colors.LocalPolkadotColors
import io.paritytech.polkadotapp.designsystem.colors.PolkadotColorsPalette
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatHeaderRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.*
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.ChatFeedContract
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.ChatFeed
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.ChatFeedToolbar
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.MessageActionMenuDropdown
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.ReactionDetailsDialog
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input.ChatInputRow
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu.ChatFeedMenu
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.LocalChatFeedTimestampAnchor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.*
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils.LocalChatFooterHeight
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

private val ACTION_MENU_BLUR_RADIUS = 25.dp

@Composable
fun ChatFeedScreen(contract: ChatFeedContract) {
    val config by contract.config.collectAsStateWithLifecycle()
    val chatInputState by contract.chatInputUiState.collectAsStateWithLifecycle()
    val chatMessagesState by contract.chatMessagesState.collectAsStateWithLifecycle()
    val menuState by contract.menuState.collectAsStateWithLifecycle()
    val displayState by contract.chatDisplay.collectAsStateWithLifecycle()
    val toolbarActions by contract.toolbarActions.collectAsStateWithLifecycle()
    val footerRenderer by contract.footerRenderer.collectAsStateWithLifecycle()
    val headerRenderer by contract.headerRenderer.collectAsStateWithLifecycle()
    val customAppearance by contract.customAppearance.collectAsStateWithLifecycle()
    val revealingMessageId by contract.revealingMessageId.collectAsStateWithLifecycle()
    val popupState = contract.popupState.collectAsStateWithLifecycle().value
    var menuLayoutInfo by remember { mutableStateOf<MessageLayoutInfo?>(null) }
    val keyboardController = LocalSoftwareKeyboardController.current

    LaunchedEffect(popupState) {
        if (popupState == null) {
            menuLayoutInfo = null
        }

        if (popupState is MessagePopUpUiState.ActionMenu) {
            keyboardController?.hide()
        }
    }

    LifecycleResumeEffect(Unit) {
        contract.setChatActive()

        onPauseOrDispose {
            contract.setChatInactive()
        }
    }

    ChatFeedScreenInternal(
        displayState = displayState,
        config = config,
        chatInputUiState = chatInputState,
        chatMessagesLoadingState = chatMessagesState,
        footerRenderer = footerRenderer,
        headerRenderer = headerRenderer,
        customAppearance = customAppearance,
        revealingMessageId = revealingMessageId,
        onMessageRevealComplete = contract::onMessageRevealComplete,
        toolbarActions = toolbarActions,
        highlightEvents = contract.highlightEvents,
        scrollToPosition = contract.scrollToPosition,
        onBackClick = contract::onBackClick,
        onMessageChange = contract::onMessageChange,
        onSendMessageClick = contract::onSendMessageClick,
        onPayClick = contract::onPayClick,
        onClearReply = contract::onClearReply,
        onClearEdit = contract::onClearEdit,
        onMessageAction = contract::onMessageAction,
        onUnreadMessageVisible = contract::onUnreadMessageVisible,
        onStartCallClick = contract::onStartCallClick,
        onMenuClick = contract::onMenuClick,
        onAttachClick = contract::onAttachClick,
        onAcceptChatRequest = contract::onAcceptChatRequest,
        onDeclineChatRequest = contract::onDeclineChatRequest,
        onUnblockUserClick = contract::onUnblockUserClick,
        isActionMenuVisible = popupState is MessagePopUpUiState.ActionMenu,
        onMessageLongPress = { message, layoutInfo ->
            menuLayoutInfo = layoutInfo
            contract.onMessageAction(MessageAction.LongPress(message))
        }
    )

    ChatFeedMenu(
        state = menuState,
        onDismiss = contract::onCloseMenu,
        onCopyUsernameClick = contract::onCopyUsernameClick,
        onLeaveChatRequest = contract::onLeaveChatRequest,
        onLeaveChatConfirm = contract::onLeaveChatConfirm,
        onBlockUserRequest = contract::onBlockUserRequest,
        onBlockUserConfirm = contract::onBlockUserConfirm
    )

    when (popupState) {
        is MessagePopUpUiState.ReactionsDetails -> {
            ReactionDetailsDialog(
                reactionDetails = popupState,
                onDismiss = { contract.onMessageAction(MessageAction.DismissActionMenu) },
                onBack = { contract.onMessageAction(MessageAction.DismissActionMenu) }
            )
        }

        is MessagePopUpUiState.ActionMenu -> {
            MessageActionMenuDropdown(
                state = popupState,
                layoutInfo = menuLayoutInfo,
                onMessageAction = contract::onMessageAction
            )
        }

        null -> {}
    }
}

@Composable
private fun ChatFeedScreenInternal(
    displayState: LoadingState<ChatDisplayUiModel>,
    config: ChatConfig,
    chatInputUiState: ChatInputUiState,
    chatMessagesLoadingState: LoadingState<ChatMessagesState>,
    footerRenderer: CustomChatFooterRenderer?,
    headerRenderer: CustomChatHeaderRenderer?,
    customAppearance: CustomChatAppearance?,
    revealingMessageId: ChatMessageId?,
    onMessageRevealComplete: (ChatMessageId) -> Unit,
    toolbarActions: ImmutableList<ChatToolbarAction>,
    highlightEvents: Flow<HighlightedMessage>,
    scrollToPosition: Flow<Int>,
    onBackClick: () -> Unit,
    onMessageChange: (String) -> Unit,
    onSendMessageClick: () -> Unit,
    onPayClick: () -> Unit,
    onAttachClick: () -> Unit,
    onClearReply: () -> Unit,
    onClearEdit: () -> Unit,
    onMessageAction: (MessageAction) -> Unit,
    onUnreadMessageVisible: (ChatMessageUiModel) -> Unit,
    onStartCallClick: (withVideo: Boolean) -> Unit,
    onMenuClick: () -> Unit,
    onAcceptChatRequest: () -> Unit,
    onDeclineChatRequest: () -> Unit,
    onUnblockUserClick: () -> Unit,
    isActionMenuVisible: Boolean,
    onMessageLongPress: (ChatMessageUiModel, MessageLayoutInfo) -> Unit
) {
    val displayData = displayState.dataOrNull
    val username = displayData?.username.orEmpty()
    val feedBlur by animateDpAsState(
        targetValue = if (isActionMenuVisible) ACTION_MENU_BLUR_RADIUS else 0.dp
    )

    var footerHeight by remember { mutableStateOf(0.dp) }

    // Messages can be replied to only when we have a regular send message input
    val canReplyToMessages = chatInputUiState is ChatInputUiState.SendMessage && !chatInputUiState.isChatRequest

    PolkadotSurface {
        customAppearance?.backgroundRenderer?.DrawBackground()
        Column(
            modifier = Modifier
                .fillMaxSize()
                .blur(feedBlur)
        ) {
            ChatToolbarThemeScope(palette = customAppearance?.toolbarPalette) {
                ChatFeedToolbar(
                    displayState = displayState,
                    toolbarActions = toolbarActions,
                    showAvatar = config.showAvatar,
                    onBack = onBackClick,
                    onStartCallClick = onStartCallClick,
                    onMenuClick = onMenuClick,
                )
            }

            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
            ) {
                chatMessagesLoadingState.onLoaded {
                    CompositionLocalProvider(LocalChatFooterHeight provides footerHeight) {
                        ChatFeed(
                            chatMessagesState = it,
                            username = username,
                            headerRenderer = headerRenderer,
                            footerRenderer = footerRenderer,
                            showTimestamps = config.showTimestamps,
                            highlightEvents = highlightEvents,
                            scrollToPosition = scrollToPosition,
                            onMessageAction = onMessageAction,
                            onUnreadMessageVisible = onUnreadMessageVisible,
                            onMessageLongPress = onMessageLongPress,
                            isInputEnabled = canReplyToMessages,
                            customBubbleStyle = customAppearance?.bubbleStyle,
                            revealingMessageId = revealingMessageId,
                            onMessageRevealComplete = onMessageRevealComplete,
                            slideInNewMessages = customAppearance != null,
                            showNewMessagesSeparator = config.showNewMessagesSeparator,
                            dateSeparatorStyle = customAppearance?.dateSeparatorStyle,
                        )
                    }
                }

                Box(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                ) {
                    ChatInputRow(
                        inputState = chatInputUiState,
                        username = username,
                        onClearReply = onClearReply,
                        onClearEdit = onClearEdit,
                        onMessageChange = onMessageChange,
                        onSendMessageClick = onSendMessageClick,
                        onPayClick = onPayClick,
                        onAttachClick = onAttachClick,
                        onAcceptChatRequest = onAcceptChatRequest,
                        onDeclineChatRequest = onDeclineChatRequest,
                        onUnblockUserClick = onUnblockUserClick,
                        onHeightChanged = { footerHeight = it },
                    )
                }
            }
        }
    }
}

@Preview
@Composable
private fun ChatFeedPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked(),
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current),
            LocalChatFeedTimestampAnchor provides System.currentTimeMillis(),
            LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
        ) {
            PolkadotSurface {
                val text = remember { mutableStateOf("") }
                ChatFeedScreenInternal(
                    displayState = LoadingState.Loaded(
                        ChatDisplayUiModel(
                            username = "Preview",
                            avatarModel = AvatarUiModel.Mock.fromName("Preview")
                        )
                    ),
                    config = ChatConfig.Default,
                    chatInputUiState = ChatInputUiState.SendMessage(
                        messageState = ChatSendMessageInputState(inputMessage = text.value),
                        isChatRequest = false
                    ),
                    chatMessagesLoadingState = LoadingState.Loaded(
                        ChatMessagesState(
                            messages = persistentListOf(
                                ChatMessageUiModel.Text(
                                    id = "1",
                                    direction = ChatMessageUiModel.Direction.INCOMING,
                                    text = "Hello!",
                                    timestamp = 1625247660000,
                                    status = ChatMessageUiModel.Status.SENT,
                                    reactions = persistentListOf(),
                                    origin = ChatMessageOrigin.User,
                                    replyPreview = null,
                                    isEdited = false
                                ),
                                ChatMessageUiModel.Text(
                                    id = "2",
                                    direction = ChatMessageUiModel.Direction.OUTGOING,
                                    text = "Hi there!",
                                    timestamp = 1625247600000,
                                    status = ChatMessageUiModel.Status.READ,
                                    reactions = persistentListOf(),
                                    origin = ChatMessageOrigin.User,
                                    replyPreview = null,
                                    isEdited = false
                                ),
                                ChatMessageUiModel.ContactAdded(
                                    id = "3",
                                    direction = ChatMessageUiModel.Direction.INCOMING,
                                    timestamp = 1625247540000,
                                    status = ChatMessageUiModel.Status.SENT,
                                    origin = ChatMessageOrigin.User
                                ),
                            )
                        )
                    ),
                    footerRenderer = null,
                    headerRenderer = null,
                    customAppearance = null,
                    revealingMessageId = null,
                    onMessageRevealComplete = {},
                    toolbarActions = persistentListOf(ChatToolbarAction.VIDEO_CALL),
                    highlightEvents = emptyFlow(),
                    scrollToPosition = emptyFlow(),
                    onBackClick = {},
                    onMessageChange = { text.value = it },
                    onSendMessageClick = {},
                    onPayClick = {},
                    onAttachClick = {},
                    onClearReply = {},
                    onClearEdit = {},
                    onMessageAction = {},
                    onUnreadMessageVisible = {},
                    onStartCallClick = {},
                    onMenuClick = {},
                    onAcceptChatRequest = {},
                    onDeclineChatRequest = {},
                    onUnblockUserClick = {},
                    isActionMenuVisible = false,
                    onMessageLongPress = { _, _ -> }
                )
            }
        }
    }
}

@Composable
private fun ChatToolbarThemeScope(
    palette: PolkadotColorsPalette?,
    content: @Composable () -> Unit,
) {
    if (palette != null) {
        CompositionLocalProvider(LocalPolkadotColors provides palette, content = content)
    } else {
        content()
    }
}
