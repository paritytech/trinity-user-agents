package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.multimedia

import android.net.Uri
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.core.net.toUri
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel.Multimedia.MultimediaType
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReplyPreview
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferDirection
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferUiState
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.LocalMultimediaMessageController
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.MultimediaMessageController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.ChatMessageContainer
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FilledMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatEditedLabel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.REVEAL_MEDIA_START_DELAY
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.TextMessageLayout
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.rememberRevealedCharCount
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.defaultTextColor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import java.util.UUID

@Composable
fun MultimediaMessage(
    modifier: Modifier,
    message: ChatMessageUiModel.Multimedia,
    showTimestamp: Boolean,
    grouping: ChatMessageGrouping,
    isHighlighted: Boolean,
    canBeReplied: Boolean,
    onMessageAction: (MessageAction) -> Unit,
    onLongPress: (MessageLayoutInfo) -> Unit,
    customBubbleStyle: ChatMessageSurfaceStyle? = null,
    isRevealing: Boolean = false,
    onRevealComplete: () -> Unit = {},
) {
    val mediaSize = calculateMediaSize(message.type)

    val controller = LocalMultimediaMessageController.current
    val transferState by controller.observe(message.id).collectAsStateWithLifecycle(null)

    val captionVisibleCharCount = rememberRevealedCharCount(
        fullText = message.text.orEmpty(),
        isRevealing = isRevealing,
        onRevealComplete = onRevealComplete,
        startDelay = REVEAL_MEDIA_START_DELAY,
    )

    ChatMessageContainer(
        modifier = modifier,
        message = message,
        grouping = grouping,
        isHighlighted = isHighlighted,
        canBeReplied = canBeReplied,
        onMessageAction = onMessageAction,
        onLongPress = onLongPress,
        reactions = message.reactions,
        replyPreview = message.replyPreview,
        surfaceStyle = customBubbleStyle ?: ChatMessageSurfaceStyle.default(message.direction),
    ) {
        Column(modifier = Modifier.width(mediaSize.width)) {
            if (message.replyPreview != null) {
                VerticalSpacer { tiny }
            }

            Box {
                when (message.type) {
                    is MultimediaType.Image -> {
                        ImagePreview(
                            modifier = Modifier
                                .size(mediaSize)
                                .clickable(
                                    enabled = message.isOpenable(),
                                    onClick = { message.uri?.let(controller::openImage) }
                                ),
                            uri = message.uri,
                            blurHash = message.blurHash
                        )
                    }

                    is MultimediaType.Video -> VideoContent(
                        modifier = Modifier.size(mediaSize),
                        uri = message.uri,
                        blurHash = message.blurHash,
                        canPlay = transferState == null
                    )
                }

                transferState?.let { state ->
                    TransferStatePill(
                        modifier = Modifier.padding(PolkadotTheme.spacings.small),
                        state = state
                    )
                }

                if (message.text == null && showTimestamp) {
                    FilledMessageTimestamp(
                        modifier = Modifier
                            .align(Alignment.BottomEnd)
                            .padding(PolkadotTheme.spacings.small),
                        message = message
                    )
                }

                TransferStateControl(
                    modifier = Modifier.align(Alignment.Center),
                    state = transferState,
                    onRedownload = { controller.redownload(message.id) },
                    onCancel = { controller.cancel(message.id) }
                )
            }

            if (message.text != null) {
                val timestampComposable = if (showTimestamp) {
                    @Composable {
                        FlatMessageTimestamp(message = message)
                    }
                } else {
                    null
                }

                val editedLabelComposable = if (message.isEdited) {
                    @Composable {
                        FlatEditedLabel(direction = message.direction)
                    }
                } else {
                    null
                }

                TextMessageLayout(
                    modifier = Modifier.fillMaxWidth(),
                    text = message.text.orEmpty(),
                    style = PolkadotTheme.typography.body.large,
                    color = customBubbleStyle?.textColor ?: message.direction.defaultTextColor,
                    timestamp = timestampComposable,
                    direction = message.direction,
                    status = editedLabelComposable,
                    visibleCharCount = captionVisibleCharCount,
                )
            }
        }
    }
}

@Preview(device = "spec:width=1080px,height=5000px,dpi=440")
@Composable
private fun MultimediaMessagePreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(PolkadotTheme.spacings.mediumIncreased)
                    .verticalScroll(rememberScrollState())
            ) {
                val messageSquareText = ChatMessageUiModel.Multimedia(
                    id = UUID.randomUUID().toString(),
                    timestamp = 0L,
                    direction = ChatMessageUiModel.Direction.INCOMING,
                    status = ChatMessageUiModel.Status.READ,
                    uri = "https://example.com/assets/image.jpg".toUri(),
                    type = MultimediaType.Image(800, 800),
                    text = "Square image with text",
                    replyPreview = null,
                    blurHash = null,
                    reactions = persistentListOf(
                        ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false),
                        ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false),
                    ),
                    origin = ChatMessageOrigin.User,
                    isEdited = false
                )

                MultimediaMessage(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(bottom = PolkadotTheme.spacings.mediumIncreased),
                    message = messageSquareText,
                    showTimestamp = true,
                    grouping = ChatMessageGrouping.Standalone,
                    isHighlighted = false,
                    canBeReplied = true,
                    onLongPress = {},
                    onMessageAction = {},
                )

                val messageWideNoText = ChatMessageUiModel.Multimedia(
                    id = UUID.randomUUID().toString(),
                    timestamp = 0L,
                    direction = ChatMessageUiModel.Direction.OUTGOING,
                    status = ChatMessageUiModel.Status.READ,
                    uri = "https://example.com/assets/image.jpg".toUri(),
                    type = MultimediaType.Image(800, 800),
                    text = "This is the text to try the message with text!",
                    replyPreview = null,
                    blurHash = null,
                    origin = ChatMessageOrigin.User,
                    reactions = persistentListOf(
                        ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false),
                        ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false),
                    ),
                    isEdited = false
                )

                CompositionLocalProvider(
                    LocalMultimediaMessageController provides previewMultimediaMessageController(
                        FileTransferUiState.InProgress(FileTransferDirection.UPLOAD, progress = 10.percents)
                    )
                ) {
                    MultimediaMessage(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(bottom = PolkadotTheme.spacings.mediumIncreased),
                        message = messageWideNoText,
                        showTimestamp = true,
                        grouping = ChatMessageGrouping.Standalone,
                        isHighlighted = false,
                        canBeReplied = true,
                        onLongPress = {},
                        onMessageAction = {},
                    )
                }

                val messageSquareNoText = ChatMessageUiModel.Multimedia(
                    id = UUID.randomUUID().toString(),
                    timestamp = 0L,
                    direction = ChatMessageUiModel.Direction.INCOMING,
                    status = ChatMessageUiModel.Status.READ,
                    uri = "https://example.com/assets/image.jpg".toUri(),
                    type = MultimediaType.Image(800, 800),
                    text = null,
                    replyPreview = null,
                    blurHash = null,
                    origin = ChatMessageOrigin.User,
                    reactions = persistentListOf(
                        ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false),
                        ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false),
                    ),
                    isEdited = false
                )

                MultimediaMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = messageSquareNoText,
                    showTimestamp = true,
                    grouping = ChatMessageGrouping.Standalone,
                    isHighlighted = false,
                    canBeReplied = true,
                    onLongPress = {},
                    onMessageAction = {},
                )

                val messageWithReply = ChatMessageUiModel.Multimedia(
                    id = UUID.randomUUID().toString(),
                    timestamp = 0L,
                    direction = ChatMessageUiModel.Direction.OUTGOING,
                    status = ChatMessageUiModel.Status.READ,
                    uri = "https://example.com/assets/image.jpg".toUri(),
                    type = MultimediaType.Image(800, 800),
                    text = "Replying with an image",
                    replyPreview = ReplyPreview(
                        messageId = "1",
                        title = "Jake.23",
                        content = ReplyPreview.Content.Text("short message short message short message")
                    ),
                    blurHash = null,
                    origin = ChatMessageOrigin.User,
                    reactions = persistentListOf(),
                    isEdited = false
                )

                MultimediaMessage(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = PolkadotTheme.spacings.mediumIncreased),
                    message = messageWithReply,
                    showTimestamp = true,
                    grouping = ChatMessageGrouping.Standalone,
                    isHighlighted = false,
                    canBeReplied = true,
                    onLongPress = {},
                    onMessageAction = {},
                )
            }
        }
    }
}

private fun previewMultimediaMessageController(state: FileTransferUiState): MultimediaMessageController =
    object : MultimediaMessageController {
        override fun observe(id: ChatMessageId): Flow<FileTransferUiState?> = flowOf(state)
        override fun redownload(messageId: ChatMessageId) = Unit
        override fun cancel(messageId: ChatMessageId) = Unit
        override fun openImage(uri: Uri) = Unit
    }
