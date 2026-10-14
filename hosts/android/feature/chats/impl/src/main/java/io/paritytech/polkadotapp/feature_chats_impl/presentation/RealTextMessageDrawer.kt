package io.paritytech.polkadotapp.feature_chats_impl.presentation

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.TextMessageDrawer
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReplyPreview
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.ReplyPreviewBubble
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.TextMessageContent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.ChatMessageSurface
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.persistentListOf
import java.util.UUID
import javax.inject.Inject

class RealTextMessageDrawer @Inject constructor(
    private val chatMessageTimeFormatter: ChatMessageTimeFormatter
) : TextMessageDrawer {
    @Composable
    override fun Draw(
        modifier: Modifier,
        text: String,
        isOutgoing: Boolean,
        timestamp: Timestamp,
        repliedTo: String?,
        repliedText: String?
    ) {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides chatMessageTimeFormatter
        ) {
            val message = ChatMessageUiModel.Text(
                id = UUID.randomUUID().toString(),
                timestamp = timestamp,
                direction = if (isOutgoing) ChatMessageUiModel.Direction.OUTGOING else ChatMessageUiModel.Direction.INCOMING,
                status = ChatMessageUiModel.Status.READ,
                origin = ChatMessageOrigin.User,
                text = text,
                replyPreview = if (repliedTo != null && repliedText != null) {
                    ReplyPreview(
                        messageId = UUID.randomUUID().toString(),
                        title = repliedTo,
                        content = ReplyPreview.Content.Text(repliedText)
                    )
                } else null,
                reactions = persistentListOf(),
                isEdited = false
            )

            ChatMessageSurface(
                modifier = modifier,
                direction = message.direction,
                grouping = ChatMessageGrouping.Standalone,
                onLongPress = {}
            ) {
                Column(
                    modifier = Modifier.width(IntrinsicSize.Max)
                ) {
                    message.replyPreview?.let {
                        VerticalSpacer { tiny }

                        ReplyPreviewBubble(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = PolkadotTheme.spacings.tiny),
                            preview = it,
                            direction = message.direction,
                            onClick = {}
                        )
                    }

                    TextMessageContent(
                        showTimestamp = true,
                        message = message,
                        text = message.text,
                        isEdited = message.isEdited
                    )
                }
            }
        }
    }
}
