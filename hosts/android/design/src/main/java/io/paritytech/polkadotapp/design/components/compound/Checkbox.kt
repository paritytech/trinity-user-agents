package io.paritytech.polkadotapp.design.components.compound

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.unit.Dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun PolkadotCheckBox(
    checked: Boolean,
    onCheckedChange: ((Boolean) -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: CheckboxColors = CheckboxDefaults.polkadotColors(),
    interactionSource: MutableInteractionSource? = null
) {
    CompositionLocalProvider(
        LocalMinimumInteractiveComponentSize provides Dp.Unspecified
    ) {
        Checkbox(
            modifier = modifier,
            checked = checked,
            onCheckedChange = onCheckedChange,
            enabled = enabled,
            colors = colors,
            interactionSource = interactionSource
        )
    }
}

@Composable
fun PolkadotTriStateCheckbox(
    state: ToggleableState,
    onClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: CheckboxColors = CheckboxDefaults.polkadotColors(),
    interactionSource: MutableInteractionSource? = null
) {
    CompositionLocalProvider(
        LocalMinimumInteractiveComponentSize provides Dp.Unspecified
    ) {
        TriStateCheckbox(
            modifier = modifier,
            state = state,
            onClick = onClick,
            enabled = enabled,
            colors = colors,
            interactionSource = interactionSource
        )
    }
}

@Composable
fun CheckboxDefaults.polkadotColors(
    checkedColor: Color = PolkadotTheme.colors.fg.primary,
    uncheckedColor: Color = PolkadotTheme.colors.fg.secondary,
    checkmarkColor: Color = PolkadotTheme.colors.bg.surface.main,
    disabledCheckedColor: Color = PolkadotTheme.colors.fg.tertiary,
    disabledUncheckedColor: Color = PolkadotTheme.colors.fg.tertiary,
    disabledIndeterminateColor: Color = PolkadotTheme.colors.fg.tertiary
) = colors(
    checkedColor = checkedColor,
    uncheckedColor = uncheckedColor,
    checkmarkColor = checkmarkColor,
    disabledCheckedColor = disabledCheckedColor,
    disabledUncheckedColor = disabledUncheckedColor,
    disabledIndeterminateColor = disabledIndeterminateColor
)
