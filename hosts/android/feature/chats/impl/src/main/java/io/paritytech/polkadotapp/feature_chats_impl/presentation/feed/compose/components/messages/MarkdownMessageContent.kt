package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.buildAnnotatedString
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatEditedLabel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.TextMessageLayout
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.rememberMarkdownText
import io.paritytech.polkadotapp.common.R as RCommon

/** A body of several thousand characters shows collapsed with "Show more" (02-DESIGN §6.1). */
private const val COLLAPSED_LENGTH = 3_000

@Composable
fun MarkdownMessageContent(
    showTimestamp: Boolean,
    message: ChatMessageUiModel,
    markdown: String,
    isEdited: Boolean,
    textColor: Color = message.direction.defaultTextColor,
) {
    val fullText = rememberMarkdownText(markdown)
    var isExpanded by rememberSaveable(message.id) { mutableStateOf(false) }
    val isCollapsed = !isExpanded && fullText.length > COLLAPSED_LENGTH
    val shownText = remember(fullText, isCollapsed) {
        if (isCollapsed) buildAnnotatedString { append(fullText.subSequence(0, COLLAPSED_LENGTH)); append("…") } else fullText
    }

    Column {
        TextMessageLayout(
            modifier = Modifier.fillMaxWidth(),
            annotatedText = shownText,
            style = PolkadotTheme.typography.body.large,
            color = textColor,
            status = if (isEdited) {
                { FlatEditedLabel(direction = message.direction) }
            } else {
                null
            },
            timestamp = if (showTimestamp) {
                { FlatMessageTimestamp(message = message) }
            } else {
                null
            },
        )

        if (isCollapsed) {
            NovaText(
                modifier = Modifier
                    .clickable { isExpanded = true }
                    .padding(horizontal = PolkadotTheme.spacings.extraMedium)
                    .padding(bottom = PolkadotTheme.spacings.small),
                text = stringResource(RCommon.string.chat_message_show_more),
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = textColor,
            )
        }
    }
}
