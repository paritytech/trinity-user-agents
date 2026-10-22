package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.design.components.avatar.AvatarModelMocks
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.DraftPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAttachmentType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatDisplayUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

internal val ChatListAvatarSize = 64.dp

@Composable
internal fun PolkadotChatListItem(
    modifier: Modifier = Modifier,
    chat: ChatListUiState.ChatItem,
    onClick: () -> Unit,
    currentTimestamp: Long,
) {
    val timestampText = chat.preview?.let { preview ->
        LocalChatMessageTimeFormatter.current.formatChatListTime(preview.timestamp, currentTimestamp)
    }.orEmpty()
    val previewBody = chat.preview?.toPreviewBody(chat.display.username)

    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(PolkadotTheme.spacings.extraMedium),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
        verticalAlignment = Alignment.Top,
    ) {
        PolkadotAvatar(
            model = chat.display.avatarModel,
            modifier = Modifier.size(ChatListAvatarSize),
        )

        Column(
            modifier = Modifier.weight(1f),
        ) {
            ChatItemHeader(
                title = chat.display.username,
                timestamp = timestampText,
                isMuted = chat.isMuted,
            )

            // TODO: group chat sender prefix — render the sender name on its own line
            // above the preview body once the domain exposes it per chat variant.
            // if (senderName != null) {
            //     NovaText(
            //         text = senderName,
            //         style = PolkadotTheme.typography.body.mediumEmphasized,
            //         color = PolkadotTheme.colors.fg.primary,
            //         maxLines = 1,
            //         overflow = TextOverflow.Ellipsis,
            //     )
            // }

            if (previewBody != null) {
                ChatItemBody(
                    preview = previewBody,
                    badge = chat.badge,
                    hasReaction = chat.hasReaction,
                )
            }
        }
    }
}

@Preview(backgroundColor = 0xFF000000, showBackground = true, widthDp = 360)
@Composable
private fun PolkadotChatListItemPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked(),
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current),
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
            ) {
                PolkadotChatListItem(
                    chat = previewChat(
                        username = "andrey.44",
                        preview = LastMessageUiModel.Text(
                            timestamp = 0L,
                            isIncoming = true,
                            message = "Text",
                        ),
                    ),
                    onClick = {},
                    currentTimestamp = 0L,
                )
                PolkadotChatListItem(
                    chat = previewChat(
                        username = "crane.55",
                        preview = LastMessageUiModel.Attachments(
                            timestamp = 0L,
                            isIncoming = true,
                            type = MessageAttachmentType.IMAGE,
                            count = 1,
                            message = null,
                        ),
                        isMuted = true,
                        hasReaction = true,
                        badge = ChatListUiState.Badge.Unread(1),
                    ),
                    onClick = {},
                    currentTimestamp = 0L,
                )
                PolkadotChatListItem(
                    chat = previewChat(
                        username = "blaze.44",
                        preview = LastMessageUiModel.Attachments(
                            timestamp = 0L,
                            isIncoming = true,
                            type = MessageAttachmentType.IMAGE,
                            count = 10,
                            message = null,
                        ),
                        isMuted = true,
                        badge = ChatListUiState.Badge.Unread(10),
                    ),
                    onClick = {},
                    currentTimestamp = 0L,
                )
                PolkadotChatListItem(
                    chat = previewChat(
                        username = "ocean.view",
                        preview = LastMessageUiModel.Attachments(
                            timestamp = 0L,
                            isIncoming = true,
                            type = MessageAttachmentType.IMAGE,
                            count = 5,
                            message = "Shots from the trip — this caption wins over the count and wraps under the icon",
                        ),
                        badge = ChatListUiState.Badge.Unread(3),
                    ),
                    onClick = {},
                    currentTimestamp = 0L,
                )
                PolkadotChatListItem(
                    chat = previewChat(
                        username = "mysticRiver.88",
                        preview = LastMessageUiModel.Call(
                            timestamp = 0L,
                            isIncoming = true,
                            purpose = ChatMessageUiModel.Call.Purpose.AUDIO_CALL,
                            state = ChatMessageUiModel.Call.State.Ended(3.minutes),
                        ),
                        badge = ChatListUiState.Badge.Unread(1),
                    ),
                    onClick = {},
                    currentTimestamp = 0L,
                )
                PolkadotChatListItem(
                    chat = previewChat(
                        username = "GameRoom",
                        preview = LastMessageUiModel.Call(
                            timestamp = 0L,
                            isIncoming = true,
                            purpose = ChatMessageUiModel.Call.Purpose.VIDEO_CALL,
                            state = ChatMessageUiModel.Call.State.Missed,
                        ),
                        badge = ChatListUiState.Badge.Unread(2),
                    ),
                    onClick = {},
                    currentTimestamp = 0L,
                )
                PolkadotChatListItem(
                    chat = previewChat(
                        username = "Quiet.Place",
                        preview = LastMessageUiModel.Call(
                            timestamp = 0L,
                            isIncoming = false,
                            purpose = ChatMessageUiModel.Call.Purpose.AUDIO_CALL,
                            state = ChatMessageUiModel.Call.State.Declined(15.seconds),
                        ),
                        badge = ChatListUiState.Badge.Unread(12),
                        hasReaction = true,
                    ),
                    onClick = {},
                    currentTimestamp = 0L,
                )
                PolkadotChatListItem(
                    chat = previewChat(
                        username = "willow.glade",
                        preview = DraftPreviewUiModel(
                            timestamp = 0L,
                            text = "Let's catch up tomorrow",
                        ),
                    ),
                    onClick = {},
                    currentTimestamp = 0L,
                )
            }
        }
    }
}

private fun previewChat(
    username: String,
    preview: ChatPreviewUiModel,
    isMuted: Boolean = false,
    hasReaction: Boolean = false,
    badge: ChatListUiState.Badge = ChatListUiState.Badge.None,
): ChatListUiState.ChatItem {
    return ChatListUiState.ChatItem(
        chatId = ChatId.fromRawValue(username.encodeToByteArray()),
        display = ChatDisplayUiModel(
            username = username,
            avatarModel = AvatarModelMocks.fromName(username),
        ),
        badge = badge,
        preview = preview,
        isMuted = isMuted,
        hasReaction = hasReaction,
    )
}
