package io.paritytech.polkadotapp.design.components.tooltip

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.tooltip.icons.TriangleDownIcon
import io.paritytech.polkadotapp.design.components.tooltip.icons.TriangleLeftIcon
import io.paritytech.polkadotapp.design.components.tooltip.icons.TriangleRightIcon
import io.paritytech.polkadotapp.design.components.tooltip.icons.TriangleUpIcon

@Composable
internal fun TooltipArrow(
    modifier: Modifier = Modifier,
    tooltipAlignment: TooltipAlignment,
    tint: Color,
) {
    NovaIcon(
        modifier = modifier,
        imageVector = when (tooltipAlignment) {
            TooltipAlignment.Top -> TriangleDownIcon
            TooltipAlignment.Bottom -> TriangleUpIcon
            TooltipAlignment.Start -> TriangleRightIcon
            TooltipAlignment.End -> TriangleLeftIcon
        },
        tint = tint,
    )
}
