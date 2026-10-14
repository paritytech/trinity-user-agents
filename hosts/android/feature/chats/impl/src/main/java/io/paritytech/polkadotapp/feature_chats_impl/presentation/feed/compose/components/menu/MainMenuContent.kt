package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.util.fastForEach
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Block
import io.paritytech.polkadotapp.design.components.icon.vectors.ContentCopy
import io.paritytech.polkadotapp.design.components.icon.vectors.Leave
import io.paritytech.polkadotapp.design.components.menu.NovaMenuOption
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import kotlinx.collections.immutable.immutableListOf
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun MainMenuContent(
    type: ChatMenuType.MainMenu,
    onDismiss: () -> Unit,
    onCopyUsernameClick: () -> Unit,
    onLeaveChatClick: () -> Unit,
    onBlockUserClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        type.actions.fastForEach { action ->
            when (action) {
                ChatMenuAction.COPY_USERNAME -> {
                    NovaMenuOption(
                        text = stringResource(RCommon.string.chat_copy_username),
                        icon = NovaIcons.ContentCopy,
                        color = PolkadotTheme.colors.fg.tertiary,
                        onClick = onCopyUsernameClick
                    )
                }

                ChatMenuAction.LEAVE_CHAT -> {
                    NovaMenuOption(
                        text = stringResource(RCommon.string.chat_leave_chat),
                        icon = NovaIcons.Leave,
                        color = PolkadotTheme.colors.fg.error,
                        onClick = onLeaveChatClick
                    )
                }

                ChatMenuAction.BLOCK_USER -> {
                    NovaMenuOption(
                        text = stringResource(RCommon.string.chat_block_user),
                        icon = NovaIcons.Block,
                        color = PolkadotTheme.colors.fg.error,
                        onClick = onBlockUserClick
                    )
                }
            }
        }

        VerticalSpacer { mediumIncreased }

        PolkadotTextButton(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = PolkadotTheme.spacings.large),
            style = PolkadotButtonStyle.ghost(),
            text = stringResource(RCommon.string.common_cancel),
            onClick = onDismiss
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun MainContentPreview() {
    PolkadotTheme {
        MainMenuContent(
            type = ChatMenuType.MainMenu(immutableListOf(ChatMenuAction.COPY_USERNAME, ChatMenuAction.LEAVE_CHAT, ChatMenuAction.BLOCK_USER)),
            onDismiss = {},
            onCopyUsernameClick = {},
            onLeaveChatClick = {},
            onBlockUserClick = {}
        )
    }
}
