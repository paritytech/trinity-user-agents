package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.dropdown.NovaDropdownMenu
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Trash
import io.paritytech.polkadotapp.design.components.menu.NovaMenuOption
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun RecentActionMenu(
    expanded: Boolean,
    onDismissRequest: () -> Unit,
    onRemoveClick: () -> Unit,
) {
    NovaDropdownMenu(
        expanded = expanded,
        onDismissRequest = onDismissRequest,
    ) {
        NovaMenuOption(
            text = stringResource(RCommon.string.chat_search_recent_menu_remove),
            icon = NovaIcons.Trash,
            color = PolkadotTheme.colors.fg.error,
            onClick = onRemoveClick,
        )
    }
}
