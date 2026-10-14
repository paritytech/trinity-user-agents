package io.paritytech.polkadotapp.design.components.compound

import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import io.paritytech.polkadotapp.design.colors.LegacyNovaStableColors
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun NovaSwitch(
    modifier: Modifier = Modifier,
    checked: Boolean,
    onCheckedChange: ((Boolean) -> Unit)?,
    enabled: Boolean = true,
) {
    CompositionLocalProvider(
        LocalMinimumInteractiveComponentSize provides Dp.Unspecified
    ) {
        Switch(
            modifier = modifier,
            checked = checked,
            onCheckedChange = onCheckedChange,
            enabled = enabled,
            colors = SwitchDefaults.colors(
                checkedThumbColor = Color.White,
                checkedTrackColor = LegacyNovaStableColors.GreenGreen400,
                checkedBorderColor = Color.Transparent,
                uncheckedThumbColor = PolkadotTheme.colors.fg.tertiary,
                uncheckedTrackColor = Color(0x1FFFFFFF),
                uncheckedBorderColor = Color.Transparent
            )
        )
    }
}
