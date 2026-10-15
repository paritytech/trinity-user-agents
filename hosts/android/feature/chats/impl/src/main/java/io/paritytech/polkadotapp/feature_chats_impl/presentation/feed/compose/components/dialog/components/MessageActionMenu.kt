package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.HorizontalDivider
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
import io.paritytech.polkadotapp.design.components.icon.vectors.Trash
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.AllowedMessageMenuAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons.EditHistory
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons.MessageStatusFailed
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ActionMenuList(
    modifier: Modifier = Modifier,
    state: MessagePopUpUiState.ActionMenu,
    onMessageAction: (MessageAction) -> Unit
) {
    if (state.allowedMenuActions.isEmpty() && !state.productActionsLoading) return

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
            state.allowedMenuActions.forEachIndexed { index, action ->
                val startsProductSection = action is AllowedMessageMenuAction.Custom &&
                    index > 0 && state.allowedMenuActions[index - 1] !is AllowedMessageMenuAction.Custom
                if (startsProductSection) {
                    HorizontalDivider(
                        modifier = Modifier.padding(vertical = PolkadotTheme.spacings.tiny),
                        thickness = PolkadotTheme.borders.default,
                        color = PolkadotTheme.colors.stroke.secondary,
                    )
                }

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

                    is AllowedMessageMenuAction.Retry -> {
                        ActionMenuItem(
                            text = stringResource(RCommon.string.common_retry),
                            icon = MessageStatusFailed,
                            onClick = { onMessageAction(MessageAction.Retry(state.message)) }
                        )
                    }

                    is AllowedMessageMenuAction.Delete -> {
                        ActionMenuItem(
                            text = stringResource(RCommon.string.common_delete),
                            icon = NovaIcons.Trash,
                            onClick = { onMessageAction(MessageAction.Delete(state.message)) },
                            isDestructive = true,
                        )
                    }

                    is AllowedMessageMenuAction.Custom -> {
                        ProductActionMenuItem(
                            action = action,
                            onClick = { onMessageAction(MessageAction.ProductMenuAction(state.message, action.actionId)) },
                        )
                    }
                }
            }

            if (state.productActionsLoading) {
                ProductActionsLoadingItem()
            }
        }
    }
}

@Composable
private fun ProductActionMenuItem(
    action: AllowedMessageMenuAction.Custom,
    onClick: () -> Unit,
) {
    val color = if (action.isDestructive) PolkadotTheme.colors.fg.error else PolkadotTheme.colors.fg.primary

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
        Box(modifier = Modifier.size(20.dp)) {
            if (action.icon != null) {
                NovaAsyncImage(
                    modifier = Modifier.size(20.dp),
                    model = action.icon,
                    contentDescription = null,
                )
            }
        }

        HorizontalSpacer { smallIncreased }

        NovaText(
            text = action.label,
            style = PolkadotTheme.typography.body.large,
            color = color
        )
    }
}

@Composable
private fun ProductActionsLoadingItem() {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.smallIncreased
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaCircularProgressIndicator(
            modifier = Modifier.size(20.dp),
            strokeWidth = 2.dp,
        )

        HorizontalSpacer { smallIncreased }

        NovaText(
            text = stringResource(RCommon.string.chat_product_menu_loading),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.tertiary
        )
    }
}

@Composable
private fun ActionMenuItem(
    text: String,
    icon: ImageVector,
    onClick: () -> Unit,
    isDestructive: Boolean = false,
) {
    val color = if (isDestructive) PolkadotTheme.colors.fg.error else PolkadotTheme.colors.fg.primary

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
            tint = color
        )

        HorizontalSpacer { smallIncreased }

        NovaText(
            text = text,
            style = PolkadotTheme.typography.body.large,
            color = color
        )
    }
}
