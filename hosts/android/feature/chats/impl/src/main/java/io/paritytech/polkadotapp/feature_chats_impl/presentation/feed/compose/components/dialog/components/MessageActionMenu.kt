package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ContentCopy
import io.paritytech.polkadotapp.design.components.icon.vectors.Edit
import io.paritytech.polkadotapp.design.components.icon.vectors.ReplyArrow
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.AllowedMessageMenuAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons.EditHistory
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ActionMenuList(
    modifier: Modifier = Modifier,
    state: MessagePopUpUiState.ActionMenu,
    onMessageAction: (MessageAction) -> Unit
) {
    if (state.allowedMenuActions.isEmpty()) return

    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.container,
        border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary)
    ) {
        Column(
            modifier = Modifier.padding(
                vertical = PolkadotTheme.spacings.small
            )
        ) {
            state.allowedMenuActions.forEach { action ->
                when (action) {
                    is AllowedMessageMenuAction.Reply -> {
                        ActionMenuItem(
                            text = stringResource(RCommon.string.common_reply),
                            icon = NovaIcons.ReplyArrow,
                            onClick = { onMessageAction(MessageAction.Reply(state.message)) }
                        )
                    }

                    is AllowedMessageMenuAction.Copy -> {
                        ActionMenuItem(
                            text = stringResource(RCommon.string.common_copy),
                            icon = NovaIcons.ContentCopy,
                            onClick = { onMessageAction(MessageAction.Copy(action.text)) }
                        )
                    }

                    is AllowedMessageMenuAction.Edit -> {
                        ActionMenuItem(
                            text = stringResource(RCommon.string.common_edit),
                            icon = NovaIcons.Edit,
                            onClick = { onMessageAction(MessageAction.Edit(state.message, action.text)) }
                        )
                    }

                    is AllowedMessageMenuAction.ViewEditHistory -> {
                        ActionMenuItem(
                            text = stringResource(RCommon.string.chat_edit_history),
                            icon = EditHistory,
                            onClick = { onMessageAction(MessageAction.ViewEditHistory(state.message)) }
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ActionMenuItem(
    text: String,
    icon: ImageVector,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.smallIncreased
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaIcon(
            modifier = Modifier.size(20.dp),
            imageVector = icon,
            tint = PolkadotTheme.colors.fg.primary
        )

        HorizontalSpacer { smallIncreased }

        NovaText(
            text = text,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary
        )
    }
}
