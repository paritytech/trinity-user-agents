package io.paritytech.polkadotapp.design.components.spacer

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.spacings.PolkadotSpacings

@Composable
inline fun VerticalSpacer(space: PolkadotSpacings.() -> Dp) {
    PolkadotTheme.spacings.apply {
        Spacer(Modifier.height(space()))
    }
}

@Composable
inline fun HorizontalSpacer(space: PolkadotSpacings.() -> Dp) {
    PolkadotTheme.spacings.apply {
        Spacer(Modifier.width(space()))
    }
}

@Deprecated(message = "Sometimes it breaks the layout. In case you need to fill the distance between widgets in Row or Column, you should use the correct weighting for these widgets rather than filling the space with empty Spacers")
@Composable
fun ColumnScope.FillerSpacer() {
    Spacer(Modifier.weight(1f))
}

@Deprecated(message = "Sometimes it breaks the layout. In case you need to fill the distance between widgets in Row or Column, you should use the correct weighting for these widgets rather than filling the space with empty Spacers")
@Composable
fun RowScope.FillerSpacer() {
    Spacer(Modifier.weight(1f))
}
