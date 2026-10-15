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
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.UriHandler
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.LifecycleStartEffect
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
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.*
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.ChatFeedContract
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.ChatFeed
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.ChatFeedToolbar
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.ChatRoomUnavailableLine
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.MessageActionMenuDropdown
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.ReactionDetailsDialog
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input.ChatActivityLine
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input.ChatCommandCatalogue
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input.ChatInputRow
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input.ChatSuggestionsStrip
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input.ProductCustomFooter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu.ChatFeedMenu
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.LocalChatFeedTimestampAnchor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.*
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductRoomAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductRoomExtrasUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductRoomUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.catalogueFor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.inputText
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils.LocalChatFooterHeight
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import io.paritytech.polkadotapp.common.R as RCommon

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
    val roomState by contract.roomState.collectAsStateWithLifecycle()
    val productRoomState by contract.productRoomState.collectAsStateWithLifecycle()
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

    LifecycleStartEffect(Unit) {
        contract.onScreenStarted()

        onStopOrDispose {
            contract.onScreenStopped()
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
        roomState = roomState,
        productRoomState = productRoomState,
        onProductRoomAction = contract::onProductRoomAction,
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
        onLoadOlderMessages = contract::onLoadOlderMessages,
        onLoadNewerMessages = contract::onLoadNewerMessages,
        onShowNewestMessages = contract::onShowNewestMessages,
        onRetryRoom = contract::onRetryRoom,
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
        onBlockUserConfirm = contract::onBlockUserConfirm,
        onDeleteMessageConfirm = contract::onDeleteMessageConfirm,
        onProductLinkConfirm = { contract.onProductRoomAction(ProductRoomAction.OpenLink(it)) },
        participants = productRoomState.participants.list,
        participantSheet = productRoomState.participantSheet,
        onProductRoomAction = contract::onProductRoomAction,
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
    roomState: ChatRoomUiState,
    productRoomState: ProductRoomUiState,
    onProductRoomAction: (ProductRoomAction) -> Unit,
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
    onLoadOlderMessages: () -> Unit,
    onLoadNewerMessages: () -> Unit,
    onShowNewestMessages: () -> Unit,
    onRetryRoom: () -> Unit,
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
                    subtitle = productRoomState.participants.count
                        ?.let { pluralStringResource(RCommon.plurals.chat_participants_count, it, it) }
                        ?: roomState.subtitle,
                    showAvatar = config.showAvatar,
                    productToolbar = productRoomState.toolbar,
                    showsParticipants = productRoomState.participants.declared,
                    onBack = onBackClick,
                    onStartCallClick = onStartCallClick,
                    onMenuClick = onMenuClick,
                    onProductToolbarClick = { onProductRoomAction(ProductRoomAction.Toolbar(it)) },
                    onParticipantsClick = { onProductRoomAction(ProductRoomAction.ShowParticipants) },
                )
            }

            if (chatMessagesLoadingState.dataOrNull?.readiness == ChatRoomReadinessUiState.UNAVAILABLE) {
                ChatRoomUnavailableLine(modifier = Modifier.fillMaxWidth(), onRetry = onRetryRoom)
            }

            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
            ) {
                val uriHandler = rememberMessageLinkHandler(confirmsLinks = roomState.confirmsLinks, onProductRoomAction)
                chatMessagesLoadingState.onLoaded {
                    CompositionLocalProvider(LocalChatFooterHeight provides footerHeight, LocalUriHandler provides uriHandler) {
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
                            emptyPlaceholder = roomState.emptyPlaceholder,
                            onLoadOlderMessages = onLoadOlderMessages,
                            onLoadNewerMessages = onLoadNewerMessages,
                            onShowNewestMessages = onShowNewestMessages,
                            authors = productRoomState.participants.authors,
                            onAuthorClick = { onProductRoomAction(ProductRoomAction.SelectParticipant(it)) },
                        )
                    }
                }

                val density = LocalDensity.current
                Column(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .onSizeChanged { footerHeight = with(density) { it.height.toDp() } }
                ) {
                    ProductRoomExtrasSlot(
                        extras = productRoomState.extras,
                        inputState = chatInputUiState,
                        onSuggestion = { onProductRoomAction(ProductRoomAction.Suggestion(it)) },
                        onCommand = { onMessageChange(it.inputText()) },
                    )

                    val customFooter = productRoomState.customFooter
                    if (customFooter != null) {
                        ProductCustomFooter(renderer = customFooter, onHeightChanged = {})
                    } else {
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
                            onHeightChanged = {},
                        )
                    }
                }
            }
        }
    }
}

/** In a room that confirms links, a tap on a link of a message asks the user to confirm the link first (02-DESIGN §6.1). */
@Composable
private fun rememberMessageLinkHandler(confirmsLinks: Boolean, onProductRoomAction: (ProductRoomAction) -> Unit): UriHandler {
    val systemHandler = LocalUriHandler.current
    val currentAction by rememberUpdatedState(onProductRoomAction)

    return remember(confirmsLinks, systemHandler) {
        if (confirmsLinks) {
            object : UriHandler {
                override fun openUri(uri: String) = currentAction(ProductRoomAction.ConfirmLink(uri))
            }
        } else {
            systemHandler
        }
    }
}

/** The activity line, the quick replies while the input is empty, and the `/` catalogue while the user types a command. */
@Composable
private fun ProductRoomExtrasSlot(
    extras: ProductRoomExtrasUiState,
    inputState: ChatInputUiState,
    onSuggestion: (String) -> Unit,
    onCommand: (ChatCommandModel) -> Unit,
) {
    val inputText = (inputState as? ChatInputUiState.SendMessage)?.messageState?.inputMessage
    val catalogue = remember(inputText, extras.commands) { extras.commands.catalogueFor(inputText.orEmpty()) }

    extras.activity?.let { ChatActivityLine(text = it) }

    if (catalogue.isNotEmpty()) {
        ChatCommandCatalogue(commands = catalogue, onSelect = onCommand)
    } else if (inputText?.isEmpty() == true && extras.suggestions.isNotEmpty()) {
        ChatSuggestionsStrip(suggestions = extras.suggestions, onSelect = onSuggestion)
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
                                    isEdited = false,
                                    isMarkdown = false
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
                                    isEdited = false,
                                    isMarkdown = false
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
                    roomState = ChatRoomUiState.PersonChat,
                    productRoomState = ProductRoomUiState.None,
                    onProductRoomAction = {},
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
                    onLoadOlderMessages = {},
                    onLoadNewerMessages = {},
                    onShowNewestMessages = {},
                    onRetryRoom = {},
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
