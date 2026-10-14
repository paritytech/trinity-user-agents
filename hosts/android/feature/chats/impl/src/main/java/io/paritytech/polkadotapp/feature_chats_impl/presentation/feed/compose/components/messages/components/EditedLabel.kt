package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Standalone "edited" label without background. Used in [TextMessageLayout] as a separate slot
 * so that the edited indicator remains visible even when the timestamp is hidden.
 */
@Composable
fun FlatEditedLabel(
    modifier: Modifier = Modifier,
    direction: ChatMessageUiModel.Direction
) {
    val contentColor = when (direction) {
        ChatMessageUiModel.Direction.INCOMING -> PolkadotTheme.colors.fg.tertiary
        ChatMessageUiModel.Direction.OUTGOING -> PolkadotTheme.colors.fg.secondaryInverted
    }

    NovaText(
        modifier = modifier,
        text = androidx.compose.ui.res.stringResource(RCommon.string.chat_message_edited),
        style = PolkadotTheme.typography.body.smallEmphasized,
        color = contentColor,
        maxLines = 1
    )
}
