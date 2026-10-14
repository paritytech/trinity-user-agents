package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDragHandler
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.common.getMaxMessageWidth
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageRevisionUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.ChatMessageSurface
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.TextMessageLayout
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.defaultAlignment
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.defaultTextColor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun MessageHistoryContent(
    type: ChatMenuType.MessageHistory
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaBottomSheetDragHandler()

        VerticalSpacer { large }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = PolkadotTheme.spacings.large)
        ) {
            CurrentRevision(type.current)

            VerticalSpacer { mediumIncreased }

            History(type.history)
        }
    }
}

@Composable
private fun CurrentRevision(current: MessageRevisionUiModel) {
    PolkadotSurface(
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.main
    ) {
        MessageHistoryItem(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased),
            historyItem = current,
            showTimestamp = true
        )
    }
}

@Composable
private fun History(history: ImmutableList<MessageRevisionUiModel>) {
    NovaText(
        text = stringResource(RCommon.string.chat_edit_history),
        style = PolkadotTheme.typography.title.large,
        color = PolkadotTheme.colors.fg.primary
    )

    VerticalSpacer { mediumIncreased }

    PolkadotSurface(
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.main
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium)
        ) {
            history.forEach {
                MessageHistoryItem(
                    modifier = Modifier.fillMaxWidth(),
                    historyItem = it,
                    showTimestamp = false
                )
            }
        }
    }
}

@Composable
private fun MessageHistoryItem(
    modifier: Modifier,
    historyItem: MessageRevisionUiModel,
    showTimestamp: Boolean
) {
    Box(
        modifier = modifier
    ) {
        val direction = ChatMessageUiModel.Direction.OUTGOING

        ChatMessageSurface(
            modifier = Modifier
                .widthIn(max = getMaxMessageWidth())
                .align(direction.defaultAlignment),
            direction = direction
        ) {
            TextMessageLayout(
                modifier = Modifier.padding(
                    vertical = PolkadotTheme.spacings.extraMedium,
                    horizontal = 14.dp
                ),
                text = historyItem.text,
                style = PolkadotTheme.typography.body.large,
                color = direction.defaultTextColor,
                direction = direction,
                timestamp = if (showTimestamp) {
                    {
                        NovaText(
                            text = LocalChatMessageTimeFormatter.current.formatMessageTime(historyItem.timestamp),
                            style = PolkadotTheme.typography.body.small,
                            color = Color(0xA8000000)
                        )
                    }
                } else {
                    null
                }
            )
        }
    }
}

@Preview
@Composable
private fun MessageHistoryContentPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            PolkadotSurface(color = PolkadotTheme.colors.bg.surface.container) {
                MessageHistoryContent(
                    type = ChatMenuType.MessageHistory(
                        current = MessageRevisionUiModel(
                            text = "Hello, this is the current message!",
                            timestamp = 0L
                        ),
                        history = persistentListOf(
                            MessageRevisionUiModel(
                                text = "Hello, this is the old message!",
                                timestamp = 0L
                            ),
                            MessageRevisionUiModel(
                                text = "Hello, world!",
                                timestamp = 0L
                            )
                        )
                    )
                )
            }
        }
    }
}
