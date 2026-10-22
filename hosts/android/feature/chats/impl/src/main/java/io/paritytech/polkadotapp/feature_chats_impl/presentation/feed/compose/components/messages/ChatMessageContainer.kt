package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.common.getMaxMessageWidth
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReplyPreview
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.MessageHighlightWrapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.ChatMessageSurface
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.MessageReactions
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.SwipeToReplyContainer
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

@Composable
fun ChatMessageContainer(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel,
    grouping: ChatMessageGrouping,
    isHighlighted: Boolean,
    canBeReplied: Boolean,
    onMessageAction: (MessageAction) -> Unit,
    onLongPress: ((MessageLayoutInfo) -> Unit)? = null,
    reactions: ImmutableList<ChatMessageUiModel.Reaction> = persistentListOf(),
    replyPreview: ReplyPreview? = null,
    surfaceStyle: ChatMessageSurfaceStyle = ChatMessageSurfaceStyle.default(message.direction),
    body: @Composable ColumnScope.() -> Unit
) {
    MessageHighlightWrapper(isHighlighted = isHighlighted) {
        Box(modifier) {
            SwipeToReplyContainer(
                enabled = canBeReplied,
                onReply = { onMessageAction(MessageAction.Reply(message)) }
            ) {
                Column(
                    modifier = Modifier
                        .align(message.direction.defaultAlignment)
                        .width(IntrinsicSize.Max)
                        .widthIn(max = getMaxMessageWidth())
                ) {
                    ChatMessageSurface(
                        direction = message.direction,
                        grouping = grouping,
                        style = surfaceStyle,
                        onLongPress = onLongPress
                    ) {
                        Column(
                            modifier = Modifier.width(IntrinsicSize.Max)
                        ) {
                            if (replyPreview != null) {
                                VerticalSpacer { tiny }

                                ReplyPreviewBubble(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(horizontal = PolkadotTheme.spacings.tiny),
                                    preview = replyPreview,
                                    direction = message.direction,
                                    onClick = { onMessageAction(MessageAction.ReplyPreviewTap(replyPreview.messageId)) }
                                )
                            }

                            body()
                        }
                    }

                    if (reactions.isNotEmpty()) {
                        val overlapDp = PolkadotTheme.spacings.tiny

                        MessageReactions(
                            modifier = Modifier
                                .fillMaxWidth()
                                .layout { measurable, constraints ->
                                    val overlapPx = overlapDp.roundToPx()
                                    val placeable = measurable.measure(constraints)
                                    layout(placeable.width, (placeable.height - overlapPx).coerceAtLeast(0)) {
                                        placeable.placeRelative(0, -overlapPx)
                                    }
                                }
                                .padding(horizontal = PolkadotTheme.spacings.extraMedium),
                            reactions = reactions,
                            onReactionClick = { emoji -> onMessageAction(MessageAction.Reaction(message, emoji)) }
                        )
                    }
                }
            }
        }
    }
}
