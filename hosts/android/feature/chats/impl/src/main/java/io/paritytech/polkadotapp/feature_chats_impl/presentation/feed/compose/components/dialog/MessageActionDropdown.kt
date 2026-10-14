package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog

import android.content.res.Configuration
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import io.paritytech.polkadotapp.common.utils.isSingleEmoji
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.components.ActionMenuList
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.components.QuickReactionsRow
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.ChatMessageContainer
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.EmojiOnlyContent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.TextMessageContent

@Composable
fun MessageActionMenuDropdown(
    state: MessagePopUpUiState.ActionMenu,
    layoutInfo: MessageLayoutInfo?,
    onMessageAction: (MessageAction) -> Unit,
) {
    val configuration = LocalConfiguration.current
    val isLandscape = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    val widthFraction = if (isLandscape) 0.6f else 1f

    val isIncoming = state.message.direction == ChatMessageUiModel.Direction.INCOMING
    val messageY = layoutInfo?.offset?.y ?: 0f

    val popupAlignment = calculatePopupAlignment(
        isIncoming = isIncoming,
        messageY = messageY
    )

    Popup(
        onDismissRequest = { onMessageAction(MessageAction.DismissActionMenu) },
        properties = PopupProperties(focusable = true)
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .clickable(
                    indication = null,
                    interactionSource = remember { MutableInteractionSource() },
                    onClick = { onMessageAction(MessageAction.DismissActionMenu) }
                ),
            contentAlignment = popupAlignment
        ) {
            Box(
                modifier = Modifier
                    .fillMaxWidth(widthFraction)
                    .padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
                    .pointerInput(Unit) {
                        detectTapGestures(onTap = {})
                    }
            ) {
                MessageActionMenuDropdownContent(
                    state = state,
                    onMessageAction = onMessageAction
                )
            }
        }
    }
}

@Composable
private fun calculatePopupAlignment(
    isIncoming: Boolean,
    messageY: Float
): Alignment {
    val configuration = LocalConfiguration.current
    val containerSize = LocalWindowInfo.current.containerSize
    val isLandscape = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    val screenHeightPx = containerSize.height.toFloat()

    val alignment = remember(isLandscape, isIncoming, messageY, screenHeightPx) {
        if (isLandscape) {
            return@remember MessageScreenPosition.Center.getAlignment(isIncoming)
        }

        return@remember MessageScreenPosition
            .from(messageY, screenHeightPx)
            .getAlignment(isIncoming)
    }

    return alignment
}

@Composable
private fun MessageActionMenuDropdownContent(
    state: MessagePopUpUiState.ActionMenu,
    onMessageAction: (MessageAction) -> Unit
) {
    val configuration = LocalConfiguration.current
    val isLandscape = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    val message = state.message
    val isIncoming = message.direction == ChatMessageUiModel.Direction.INCOMING
    val columnAlignment = if (isIncoming) Alignment.Start else Alignment.End

    Column(
        modifier = Modifier
            .padding(vertical = if (isLandscape) PolkadotTheme.spacings.mediumIncreased else 48.dp),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        horizontalAlignment = columnAlignment
    ) {
        if (state.canLeaveReactions) {
            Box(modifier = Modifier.fillMaxWidth()) {
                QuickReactionsRow(
                    state = state,
                    onMessageAction = onMessageAction
                )
            }
        }

        (message as? ChatMessageUiModel.Text)?.let { textMessage ->
            val isSingleEmoji = remember(textMessage.text) { textMessage.text.isSingleEmoji() }
            val surfaceStyle = if (isSingleEmoji) {
                ChatMessageSurfaceStyle.Transparent
            } else {
                ChatMessageSurfaceStyle.default(textMessage.direction)
            }

            Box(
                modifier = Modifier
                    .weight(1f, fill = false)
                    .verticalScroll(rememberScrollState())
            ) {
                ChatMessageContainer(
                    message = textMessage,
                    grouping = ChatMessageGrouping.Standalone,
                    isHighlighted = false,
                    canBeReplied = false,
                    onMessageAction = { },
                    replyPreview = textMessage.replyPreview,
                    surfaceStyle = surfaceStyle
                ) {
                    if (isSingleEmoji) {
                        EmojiOnlyContent(
                            message = textMessage,
                            showTimestamp = true,
                        )
                    } else {
                        TextMessageContent(
                            showTimestamp = true,
                            message = textMessage,
                            text = textMessage.text,
                            isEdited = textMessage.isEdited
                        )
                    }
                }
            }
        }

        Box(modifier = Modifier.fillMaxWidth()) {
            ActionMenuList(
                modifier = Modifier
                    .width(IntrinsicSize.Max)
                    .widthIn(min = 240.dp)
                    .align(if (isIncoming) Alignment.CenterStart else Alignment.CenterEnd),
                state = state,
                onMessageAction = onMessageAction
            )
        }
    }
}

private sealed class MessageScreenPosition {
    abstract fun getAlignment(isIncoming: Boolean): Alignment

    data object Top : MessageScreenPosition() {
        override fun getAlignment(isIncoming: Boolean) =
            if (isIncoming) Alignment.TopStart else Alignment.TopEnd
    }

    data object Center : MessageScreenPosition() {
        override fun getAlignment(isIncoming: Boolean) =
            if (isIncoming) Alignment.CenterStart else Alignment.CenterEnd
    }

    data object Bottom : MessageScreenPosition() {
        override fun getAlignment(isIncoming: Boolean) =
            if (isIncoming) Alignment.BottomStart else Alignment.BottomEnd
    }

    companion object {
        private const val TOP_THRESHOLD = 0.30f
        private const val BOTTOM_THRESHOLD = 0.70f

        fun from(messageY: Float, screenHeight: Float): MessageScreenPosition {
            val ratio = if (screenHeight > 0) messageY / screenHeight else 0.5f

            return when {
                ratio < TOP_THRESHOLD -> Top
                ratio > BOTTOM_THRESHOLD -> Bottom
                else -> Center
            }
        }
    }
}
