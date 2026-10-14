package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReplyPreview
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.AttachFileButton
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.ChatInputField
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.PayButton
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.paymentSubtitle
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatInputUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatRequestAnswerProgress
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatSendMessageInputState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.InputMessageRelation
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.isNotNone
import io.paritytech.polkadotapp.common.R as RCommon

private val ChatInputMinHeight = 48.dp

@Composable
internal fun ChatInputRow(
    inputState: ChatInputUiState,
    username: String,
    onClearReply: () -> Unit,
    onClearEdit: () -> Unit,
    onMessageChange: (String) -> Unit,
    onSendMessageClick: () -> Unit,
    onPayClick: () -> Unit,
    onAttachClick: () -> Unit,
    onAcceptChatRequest: () -> Unit,
    onDeclineChatRequest: () -> Unit,
    onUnblockUserClick: () -> Unit,
    onHeightChanged: (Dp) -> Unit,
) {
    val density = LocalDensity.current
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .onSizeChanged { size -> onHeightChanged(with(density) { size.height.toDp() }) },
        brush = footerBackgroundBrush(inputState)
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .imePadding()
        ) {
            when (inputState) {
                is ChatInputUiState.SendMessage -> {
                    Column {
                        if (inputState.isChatRequest) {
                            InviteToChatLabel(username = username)
                            VerticalSpacer { small }
                        }

                        SendMessageInput(
                            messageState = inputState.messageState,
                            showPayButton = inputState.showPayButton,
                            showAttachmentButton = inputState.showAttachButton,
                            onClearReply = onClearReply,
                            onClearEdit = onClearEdit,
                            onMessageChange = onMessageChange,
                            onSendMessageClick = onSendMessageClick,
                            onPayClick = onPayClick,
                            onAttachClick = onAttachClick
                        )
                    }
                }

                is ChatInputUiState.AcceptChatRequest -> {
                    AcceptChatRequestInput(
                        username = username,
                        answerProgress = inputState.answerProgress,
                        onAccept = onAcceptChatRequest,
                        onDecline = onDeclineChatRequest
                    )
                }

                is ChatInputUiState.WaitChatRequestApproval -> {
                    WaitingForAcceptLabel(username = username)
                }

                is ChatInputUiState.Hidden -> {
                    // Show nothing
                }

                ChatInputUiState.PeerLeft -> {
                    ChatFooterLeave(username = username)
                }

                ChatInputUiState.UnblockUser -> {
                    UnblockUserInput(
                        username = username,
                        onUnblock = onUnblockUserClick
                    )
                }
            }
        }
    }
}

@Composable
private fun footerBackgroundBrush(state: ChatInputUiState): Brush = when (state) {
    is ChatInputUiState.SendMessage -> Brush.verticalGradient(
        listOf(
            PolkadotTheme.colors.gradient.navigationOverlayEnd,
            PolkadotTheme.colors.gradient.navigationOverlayStart
        )
    )

    is ChatInputUiState.AcceptChatRequest -> SolidColor(PolkadotTheme.colors.bg.surface.container)

    is ChatInputUiState.Hidden,
    is ChatInputUiState.PeerLeft,
    is ChatInputUiState.UnblockUser,
    is ChatInputUiState.WaitChatRequestApproval -> SolidColor(Color.Transparent)
}

@Composable
private fun SendMessageInput(
    messageState: ChatSendMessageInputState,
    showPayButton: Boolean,
    showAttachmentButton: Boolean,
    onClearReply: () -> Unit,
    onClearEdit: () -> Unit,
    onMessageChange: (String) -> Unit,
    onSendMessageClick: () -> Unit,
    onPayClick: () -> Unit,
    onAttachClick: () -> Unit
) {
    val bubbleShape =
        if (messageState.relation is InputMessageRelation.None)
            PolkadotTheme.shapes.large
        else RoundedCornerShape(
            topStart = PolkadotTheme.radii.mediumIncreased,
            topEnd = PolkadotTheme.radii.mediumIncreased,
            bottomStart = PolkadotTheme.radii.large,
            bottomEnd = PolkadotTheme.radii.large
        )

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.small),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.Bottom
    ) {
        if (showAttachmentButton) {
            AttachFileButton(onClick = onAttachClick)
        }

        if (showPayButton) {
            PayButton(onClick = onPayClick)
        }

        PolkadotSurface(
            modifier = Modifier
                .weight(1f)
                .heightIn(min = ChatInputMinHeight),
            color = PolkadotTheme.colors.bg.action.secondary,
            shape = bubbleShape
        ) {
            Column {
                val relation = messageState.relation

                RelationBanner(relation, onClearEdit, onClearReply)

                ChatInputField(
                    modifier = Modifier
                        .fillMaxWidth()
                        .animateContentSize(),
                    text = messageState.inputMessage,
                    onTextChanged = onMessageChange,
                    onSendAction = onSendMessageClick
                )
            }
        }
    }
}

@Composable
private fun ColumnScope.RelationBanner(
    relation: InputMessageRelation,
    onClearEdit: () -> Unit,
    onClearReply: () -> Unit
) {
    var lastActiveRelation by remember { mutableStateOf(relation) }
    if (relation.isNotNone()) {
        lastActiveRelation = relation
    }

    AnimatedVisibility(visible = relation.isNotNone()) {
        val bannerModifier = Modifier.padding(
            top = PolkadotTheme.spacings.tiny,
            start = PolkadotTheme.spacings.tiny,
            end = PolkadotTheme.spacings.tiny
        )

        when (val active = lastActiveRelation) {
            is InputMessageRelation.Edit -> TitledBanner(
                modifier = bannerModifier,
                title = stringResource(RCommon.string.common_edit),
                subtitle = AnnotatedString(active.originalText),
                onClose = onClearEdit
            )

            is InputMessageRelation.Reply -> ReplyBanner(
                modifier = bannerModifier,
                preview = active.preview,
                onClose = onClearReply
            )

            InputMessageRelation.None -> Unit
        }
    }
}

@Composable
private fun ReplyBanner(
    preview: ReplyPreview,
    onClose: () -> Unit,
    modifier: Modifier = Modifier
) {
    val subtitle = when (val content = preview.content) {
        is ReplyPreview.Content.Text -> AnnotatedString(content.text)
        is ReplyPreview.Content.Image ->
            AnnotatedString(content.caption ?: stringResource(RCommon.string.chat_attachment_name_image))
        is ReplyPreview.Content.Video ->
            AnnotatedString(content.caption ?: stringResource(RCommon.string.chat_attachment_name_video))
        is ReplyPreview.Content.File -> AnnotatedString(content.caption ?: content.fileName)
        is ReplyPreview.Content.Payment -> content.paymentSubtitle(PolkadotTheme.typography.body.smallEmphasized)
    }

    TitledBanner(
        modifier = modifier,
        title = stringResource(RCommon.string.chat_reply_to, preview.title),
        subtitle = subtitle,
        onClose = onClose
    )
}

@Composable
private fun TitledBanner(
    title: String,
    subtitle: AnnotatedString,
    onClose: () -> Unit,
    modifier: Modifier = Modifier
) {
    PolkadotSurface(
        modifier = modifier.fillMaxWidth(),
        color = PolkadotTheme.colors.bg.surface.nested,
        shape = PolkadotTheme.shapes.extraMedium
    ) {
        Row(
            modifier = Modifier.height(IntrinsicSize.Min)
        ) {
            Box(
                modifier = Modifier
                    .width(PolkadotTheme.borders.large)
                    .fillMaxHeight()
                    .background(PolkadotTheme.colors.stroke.tertiary)
            )

            Column(
                modifier = Modifier
                    .weight(1f)
                    .padding(
                        horizontal = PolkadotTheme.spacings.extraMedium,
                        vertical = PolkadotTheme.spacings.small
                    )
            ) {
                NovaText(
                    text = title,
                    style = PolkadotTheme.typography.title.tinyEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )

                NovaText(
                    text = subtitle,
                    style = PolkadotTheme.typography.body.smallEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis
                )
            }

            Box(
                modifier = Modifier.padding(
                    top = PolkadotTheme.spacings.tiny,
                    end = PolkadotTheme.spacings.tiny
                )
            ) {
                PolkadotIconButton(
                    icon = NovaIcons.Close,
                    onClick = onClose,
                    style = PolkadotButtonStyle.ghost(),
                    size = PolkadotIconButtonSize.tiny(),
                    shape = PolkadotTheme.shapes.full
                )
            }
        }
    }
}

@Preview
@Composable
private fun ChatInputRowPreview() {
    PolkadotTheme {
        ChatInputRow(
            inputState = ChatInputUiState.SendMessage(
                messageState = ChatSendMessageInputState(inputMessage = "Hi! asdaksjd"),
                isChatRequest = false
            ),
            username = "Julius.87",
            onHeightChanged = {},
            onClearReply = {},
            onClearEdit = {},
            onMessageChange = { },
            onSendMessageClick = { },
            onPayClick = { },
            onAttachClick = { },
            onAcceptChatRequest = {},
            onDeclineChatRequest = {},
            onUnblockUserClick = {}
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatInputRowPreview_WithReply() {
    PolkadotTheme {
        ChatInputRow(
            inputState = ChatInputUiState.SendMessage(
                messageState = ChatSendMessageInputState(
                    inputMessage = "Hi! asdaksjd\nsecond line",
                    relation = InputMessageRelation.Reply(
                        ReplyPreview(
                            messageId = "1",
                            title = "John Doe",
                            content = ReplyPreview.Content.Text(
                                "This is a sample reply message to demonstrate the reply header in the chat input row."
                            )
                        )
                    )
                ),
                isChatRequest = false,
                showAttachButton = true,
                showPayButton = true
            ),
            username = "John Doe",
            onHeightChanged = {},
            onClearReply = {},
            onClearEdit = {},
            onMessageChange = { },
            onSendMessageClick = { },
            onPayClick = { },
            onAttachClick = { },
            onAcceptChatRequest = {},
            onDeclineChatRequest = {},
            onUnblockUserClick = {}
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatInputRowPreview_WithEdit() {
    PolkadotTheme {
        ChatInputRow(
            inputState = ChatInputUiState.SendMessage(
                messageState = ChatSendMessageInputState(
                    inputMessage = "Edited message text",
                    relation = InputMessageRelation.Edit(
                        messageId = "1",
                        originalText = "Original message that is being edited"
                    )
                ),
                isChatRequest = false
            ),
            username = "Julius.87",
            onHeightChanged = {},
            onClearReply = {},
            onClearEdit = {},
            onMessageChange = { },
            onSendMessageClick = { },
            onPayClick = { },
            onAttachClick = { },
            onAcceptChatRequest = {},
            onDeclineChatRequest = {},
            onUnblockUserClick = {}
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatInputRowPreview_InviteToChat() {
    PolkadotTheme {
        ChatInputRow(
            inputState = ChatInputUiState.SendMessage(
                messageState = ChatSendMessageInputState(inputMessage = ""),
                isChatRequest = true
            ),
            username = "Julius.87",
            onHeightChanged = {},
            onClearReply = {},
            onClearEdit = {},
            onMessageChange = { },
            onSendMessageClick = { },
            onPayClick = { },
            onAttachClick = { },
            onAcceptChatRequest = {},
            onDeclineChatRequest = {},
            onUnblockUserClick = {}
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatInputRowPreview_AcceptRequest() {
    PolkadotTheme {
        ChatInputRow(
            inputState = ChatInputUiState.AcceptChatRequest(ChatRequestAnswerProgress.None),
            username = "Maxwell.42",
            onHeightChanged = {},
            onClearReply = {},
            onClearEdit = {},
            onMessageChange = { },
            onSendMessageClick = { },
            onPayClick = { },
            onAttachClick = { },
            onAcceptChatRequest = {},
            onDeclineChatRequest = {},
            onUnblockUserClick = {}
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatInputRowPreview_WaitingForAccept() {
    PolkadotTheme {
        ChatInputRow(
            inputState = ChatInputUiState.WaitChatRequestApproval,
            username = "Julius.87",
            onHeightChanged = {},
            onClearReply = {},
            onClearEdit = {},
            onMessageChange = { },
            onSendMessageClick = { },
            onPayClick = { },
            onAttachClick = { },
            onAcceptChatRequest = {},
            onDeclineChatRequest = {},
            onUnblockUserClick = {}
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatInputRowPreview_PeerLeft() {
    PolkadotTheme {
        ChatInputRow(
            inputState = ChatInputUiState.PeerLeft,
            username = "Julius.87",
            onHeightChanged = {},
            onClearReply = {},
            onClearEdit = {},
            onMessageChange = { },
            onSendMessageClick = { },
            onPayClick = { },
            onAttachClick = { },
            onAcceptChatRequest = {},
            onDeclineChatRequest = {},
            onUnblockUserClick = {}
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatInputRowPreview_UnblockUser() {
    PolkadotTheme {
        ChatInputRow(
            inputState = ChatInputUiState.UnblockUser,
            username = "Julius.87",
            onHeightChanged = {},
            onClearReply = {},
            onClearEdit = {},
            onMessageChange = { },
            onSendMessageClick = { },
            onPayClick = { },
            onAcceptChatRequest = {},
            onDeclineChatRequest = {},
            onUnblockUserClick = {},
            onAttachClick = {}
        )
    }
}
