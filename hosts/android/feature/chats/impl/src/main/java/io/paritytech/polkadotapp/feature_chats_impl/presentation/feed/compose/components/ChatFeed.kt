package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableStateSetOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatHeaderRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.MessageDrawingContext
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.asAnyRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatDateSeparatorStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.HighlightedMessage
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.isUnread
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.*
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.CompactionUnavailableMessage
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.ContactAddedMessage
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.PaymentMessage
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.TextMessage
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.UnsupportedMessage
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.multimedia.MultimediaMessage
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.separators.GroupDateSeparator
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.separators.NewMessagesSeparator
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMessagesState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils.LocalChatFooterHeight
import kotlinx.coroutines.flow.Flow
import kotlin.math.abs

private const val GROUPING_TIME_LIMIT_MS = 60_000L
private const val HEADER_INFO_KEY = "chat_header_info"

private val SLIDE_IN_OFFSET = 24.dp
private const val SLIDE_IN_DURATION_MS = 500

@Composable
internal fun ChatFeed(
    chatMessagesState: ChatMessagesState,
    username: String,
    headerRenderer: CustomChatHeaderRenderer?,
    footerRenderer: CustomChatFooterRenderer?,
    showTimestamps: Boolean,
    isInputEnabled: Boolean,
    highlightEvents: Flow<HighlightedMessage>,
    scrollToPosition: Flow<Int>,
    onMessageAction: (MessageAction) -> Unit,
    onMessageLongPress: (ChatMessageUiModel, MessageLayoutInfo) -> Unit,
    onUnreadMessageVisible: (ChatMessageUiModel) -> Unit,
    customBubbleStyle: ChatMessageSurfaceStyle? = null,
    revealingMessageId: ChatMessageId? = null,
    onMessageRevealComplete: (ChatMessageId) -> Unit = {},
    slideInNewMessages: Boolean = false,
    showNewMessagesSeparator: Boolean = true,
    dateSeparatorStyle: ChatDateSeparatorStyle? = null,
) {
    val footerHeight = LocalChatFooterHeight.current
    var initialMessageIds by remember { mutableStateOf<Set<ChatMessageId>?>(null) }
    LaunchedEffect(Unit) {
        initialMessageIds = chatMessagesState.messages.map { it.id }.toSet()
    }
    // Survives item disposal so a slid-in message doesn't replay its entry when scrolled away and back.
    val animatedMessageIds = remember { mutableStateSetOf<ChatMessageId>() }
    // TODO: This is a quick workaround to account for items other than message items when calculating sticky offset.
    //  When you will add a new item type - please consider more generic approach, that will generically isolate message indices
    //  without a need to rely on assumptions about items relative positioning (e.g. that footer always goes before all the messages)
    val itemIndexOffset = if (footerRenderer != null) 1 else 0

    val lazyListState = createLazyListStateForMessagesState(chatMessagesState, itemIndexOffset)

    val currentHighlightId = rememberHighlightedMessageId(highlightEvents, lazyListState)
    ScrollDownUponNewMessageHandler(chatMessagesState.messages, lazyListState)

    scrollToPosition.collectAsEffect { _, position ->
        lazyListState.animateScrollToItem(position)
    }

    // A styled (Prizes) date separator shows the absolute date instead of "Today"/"Yesterday".
    val useRelativeDateLabels = dateSeparatorStyle == null
    val stickyHeaderInfoState = rememberStickyHeaderInfo(
        lazyListState = lazyListState,
        messages = chatMessagesState.messages,
        itemIndexOffset = itemIndexOffset,
        useRelativeLabels = useRelativeDateLabels,
    )

    Box(
        modifier = Modifier.fillMaxSize()
    ) {
        LazyColumn(
            modifier = Modifier.fillMaxWidth(),
            contentPadding = PaddingValues(
                top = PolkadotTheme.spacings.mediumIncreased,
                bottom = PolkadotTheme.spacings.mediumIncreased + footerHeight
            ),
            state = lazyListState,
            reverseLayout = true
        ) {
            showFooterIfNeeded(footerRenderer)

            itemsIndexed(
                items = chatMessagesState.messages,
                key = { _, message -> message.id }
            ) { index, message ->
                val isUnread = message.isUnread()
                LaunchedEffect(isUnread) {
                    if (isUnread) {
                        onUnreadMessageVisible(message)
                    }
                }

                val isHighlighted = message.id == currentHighlightId
                val isRevealing = message.id == revealingMessageId

                val newerMessage = chatMessagesState.messages.getOrNull(index - 1)
                val olderMessage = chatMessagesState.messages.getOrNull(index + 1)

                val showGroupDateSeparator = shouldShowDateSeparator(message.timestamp, olderMessage?.timestamp)

                val spacing = calculateSpacing(
                    previousMessage = newerMessage,
                    currentMessage = message
                )

                val grouping = ChatMessageGrouping(
                    isTopAttached = areGrouped(message, olderMessage),
                    isBottomAttached = areGrouped(message, newerMessage)
                )

                val isNewMessage = slideInNewMessages &&
                    initialMessageIds?.contains(message.id) == false &&
                    message.id !in animatedMessageIds

                val messageModifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
                    .padding(bottom = spacing)

                AnimatedMessageContainer(
                    animate = isNewMessage,
                    onAnimated = { animatedMessageIds.add(message.id) },
                ) {
                    when (message) {
                        is ChatMessageUiModel.ContactAdded -> ContactAddedMessage(
                            modifier = messageModifier,
                            message = message,
                            username = username
                        )

                        is ChatMessageUiModel.Text -> TextMessage(
                            modifier = messageModifier,
                            message = message,
                            showTimestamp = showTimestamps,
                            grouping = grouping,
                            isHighlighted = isHighlighted,
                            onMessageAction = onMessageAction,
                            onLongPress = { layoutInfo -> onMessageLongPress(message, layoutInfo) },
                            canBeReplied = isInputEnabled,
                            customBubbleStyle = customBubbleStyle,
                            isRevealing = isRevealing,
                            onRevealComplete = { onMessageRevealComplete(message.id) },
                        )

                        is ChatMessageUiModel.CoinagePayment -> PaymentMessage(
                            modifier = messageModifier,
                            message = message,
                            grouping = grouping,
                            isHighlighted = isHighlighted,
                            username = username,
                            canBeReplied = isInputEnabled,
                            onMessageAction = onMessageAction,
                            onLongPress = { layoutInfo -> onMessageLongPress(message, layoutInfo) },
                            customBubbleStyle = customBubbleStyle,
                        )

                        is ChatMessageUiModel.Multimedia -> MultimediaMessage(
                            modifier = messageModifier,
                            message = message,
                            showTimestamp = showTimestamps,
                            grouping = grouping,
                            isHighlighted = isHighlighted,
                            canBeReplied = isInputEnabled,
                            onMessageAction = onMessageAction,
                            onLongPress = { layoutInfo -> onMessageLongPress(message, layoutInfo) },
                            customBubbleStyle = customBubbleStyle,
                            isRevealing = isRevealing,
                            onRevealComplete = { onMessageRevealComplete(message.id) },
                        )

                        is ChatMessageUiModel.Unsupported -> UnsupportedMessage(
                            modifier = messageModifier,
                            message = message,
                            showTimestamp = showTimestamps,
                            grouping = grouping,
                            isHighlighted = isHighlighted,
                            customBubbleStyle = customBubbleStyle,
                        )

                        is ChatMessageUiModel.CompactionUnavailable -> CompactionUnavailableMessage(
                            modifier = messageModifier,
                            message = message,
                            showTimestamp = showTimestamps,
                            grouping = grouping,
                            isHighlighted = isHighlighted,
                            customBubbleStyle = customBubbleStyle,
                        )

                        is ChatMessageUiModel.File -> FileMessage(
                            modifier = messageModifier,
                            message = message,
                            showTimestamp = showTimestamps,
                            grouping = grouping,
                            isHighlighted = isHighlighted,
                            customBubbleStyle = customBubbleStyle,
                        )

                        is ChatMessageUiModel.ChatRequest -> ChatRequestMessage(
                            modifier = messageModifier,
                            message = message,
                            showTimestamp = showTimestamps,
                            grouping = grouping,
                            isHighlighted = isHighlighted,
                            onMessageAction = onMessageAction,
                            onLongPress = { layoutInfo -> onMessageLongPress(message, layoutInfo) },
                            canBeReplied = isInputEnabled,
                            customBubbleStyle = customBubbleStyle,
                        )

                        is ChatMessageUiModel.ChatAccepted -> ChatAcceptedMessage(
                            modifier = messageModifier,
                            message = message
                        )

                        is ChatMessageUiModel.Call -> CallMessage(
                            modifier = messageModifier,
                            message = message,
                            grouping = grouping,
                            isHighlighted = isHighlighted,
                            onMessageAction = onMessageAction,
                            customBubbleStyle = customBubbleStyle,
                        )

                        is ChatMessageUiModel.Custom<*> -> {
                            val drawingContext = MessageDrawingContext(
                                grouping = grouping,
                                messageModifier = messageModifier,
                            )

                            @Suppress("UNCHECKED_CAST")
                            message.renderer.asAnyRenderer()
                                .DrawMessage(message as ChatMessageUiModel.Custom<Any?>, drawingContext)
                        }
                    }
                }

                if (showGroupDateSeparator) {
                    val anchorTimestamp = LocalChatFeedTimestampAnchor.current
                    GroupDateSeparator(
                        date = LocalTimeFormatter.current.formatChatDateSeparator(
                            message.timestamp,
                            anchorTimestamp,
                            useRelativeLabels = useRelativeDateLabels,
                        ),
                        isHidden = stickyHeaderInfoState.value == null || stickyHeaderInfoState.value?.messageIndex == index,
                        style = dateSeparatorStyle,
                    )
                }

                if (showNewMessagesSeparator && message.id == chatMessagesState.firstNewMessageInfo?.messageId) {
                    NewMessagesSeparator()
                }
            }

            showHeaderIfNeeded(headerRenderer = headerRenderer)
        }

        FloatingDateLabelOverlay(
            stickyHeaderInfo = stickyHeaderInfoState,
            lazyListState = lazyListState,
            style = dateSeparatorStyle,
        )

        ScrollToNewButton(
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(bottom = footerHeight),
            lazyListState = lazyListState,
            unreadCounter = chatMessagesState.unreadCounter,
            containerColor = dateSeparatorStyle?.backgroundColor,
            iconTint = dateSeparatorStyle?.textColor,
        )
    }
}

@Composable
private fun createLazyListStateForMessagesState(chatMessagesState: ChatMessagesState, indexOffset: Int): LazyListState {
    val firstInitialIndex = chatMessagesState.firstNewMessageInfo?.let { it.index + indexOffset } ?: 0

    val initialScrollOffset = if (chatMessagesState.firstNewMessageInfo != null) {
        -LocalWindowInfo.current.containerSize.height / 2
    } else {
        0
    }

    return rememberLazyListState(
        initialFirstVisibleItemIndex = firstInitialIndex,
        initialFirstVisibleItemScrollOffset = initialScrollOffset
    )
}

private fun LazyListScope.showFooterIfNeeded(
    footerRenderer: CustomChatFooterRenderer?,
) {
    footerRenderer?.let { renderer ->
        item {
            renderer.drawFooter()
            VerticalSpacer { mediumIncreased }
        }
    }
}

private fun LazyListScope.showHeaderIfNeeded(
    headerRenderer: CustomChatHeaderRenderer?
) {
    headerRenderer?.let { renderer ->
        item(key = HEADER_INFO_KEY) {
            renderer.DrawHeader()
        }
    }
}

@Composable
private fun calculateSpacing(
    previousMessage: ChatMessageUiModel?,
    currentMessage: ChatMessageUiModel
): Dp {
    if (previousMessage == null) return 0.dp
    val areSameDirection = previousMessage.direction == currentMessage.direction

    val isLessThanOneMinute = isWithinGroupingTime(previousMessage, currentMessage)

    return if (areSameDirection && isLessThanOneMinute) {
        PolkadotTheme.spacings.tiny
    } else {
        PolkadotTheme.spacings.small
    }
}

private fun areGrouped(
    current: ChatMessageUiModel,
    neighbor: ChatMessageUiModel?
): Boolean {
    if (neighbor == null) return false
    if (neighbor.direction != current.direction) return false
    return abs(neighbor.timestamp - current.timestamp) < GROUPING_TIME_LIMIT_MS
}

private fun isWithinGroupingTime(
    newerMessage: ChatMessageUiModel,
    olderMessage: ChatMessageUiModel
): Boolean {
    val timeDifference = newerMessage.timestamp - olderMessage.timestamp
    return timeDifference < GROUPING_TIME_LIMIT_MS
}

// Bespoke entry transition rather than LazyListScope.animateItem(): a freshly revealed bot message needs a
// one-time slide-up + fade (gated by animatedMessageIds so it never replays on scroll); animateItem() only
// fades and re-runs on every reappearance.
@Composable
private fun AnimatedMessageContainer(
    animate: Boolean,
    onAnimated: () -> Unit,
    content: @Composable () -> Unit,
) {
    val density = LocalDensity.current
    val initialOffsetPx = with(density) { SLIDE_IN_OFFSET.roundToPx() }
    var visible by remember { mutableStateOf(!animate) }
    LaunchedEffect(Unit) {
        if (animate) {
            onAnimated()
            visible = true
        }
    }
    AnimatedVisibility(
        visible = visible,
        enter = slideInVertically(
            animationSpec = tween(durationMillis = SLIDE_IN_DURATION_MS, easing = FastOutSlowInEasing),
            initialOffsetY = { initialOffsetPx },
        ) + fadeIn(
            animationSpec = tween(durationMillis = SLIDE_IN_DURATION_MS, easing = FastOutSlowInEasing),
        ),
    ) {
        content()
    }
}
