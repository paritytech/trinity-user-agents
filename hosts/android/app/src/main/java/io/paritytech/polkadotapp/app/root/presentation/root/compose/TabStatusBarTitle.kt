package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.style.TextOverflow
import io.paritytech.polkadotapp.app.root.presentation.main.compose.title
import io.paritytech.polkadotapp.common.presentation.statusBar.IdBarPlate
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun TabStatusBarTitle(tab: BottomTab) {
    IdBarPlate(
        highlighted = false,
        contentPadding = PaddingValues(horizontal = PolkadotTheme.spacings.smallIncreased),
    ) {
        NovaText(
            text = tab.title(),
            style = PolkadotTheme.typography.title.small,
            color = PolkadotTheme.colors.fg.primary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
