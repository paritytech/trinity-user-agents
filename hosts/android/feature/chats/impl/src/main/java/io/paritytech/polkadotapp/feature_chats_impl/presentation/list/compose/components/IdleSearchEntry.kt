package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.topbar.PolkadotSearchFieldButton
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * A tappable, non-editable stand-in for the search field. It is hidden above the chat list until
 * the user drags down, and tapping it switches the screen into the real search app bar rather
 * than accepting typed input itself.
 */
@Composable
internal fun IdleSearchEntry(onClick: () -> Unit) {
    PolkadotSearchFieldButton(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.small),
        placeholder = stringResource(RCommon.string.common_search),
        onClick = onClick,
    )
}
