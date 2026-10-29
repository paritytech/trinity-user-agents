package io.paritytech.polkadotapp.app.root.presentation.main.compose.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.tooltip.NonFocusablePopupProperties
import io.paritytech.polkadotapp.design.components.tooltip.PolkadotTooltip
import io.paritytech.polkadotapp.design.components.tooltip.PolkadotTooltipContent
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.launch
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ScannerTooltip(
    tooltipVisible: Boolean,
    onTooltipDismiss: () -> Unit,
) {
    PolkadotTooltip(
        expanded = tooltipVisible,
        onDismiss = onTooltipDismiss,
        arrowVisible = true,
        shape = PolkadotTheme.shapes.tiny,
        properties = NonFocusablePopupProperties
    ) {
        PolkadotTooltipContent(
            title = stringResource(RCommon.string.bottom_nav_scanner_tooltip_title),
            message = stringResource(RCommon.string.bottom_nav_scanner_tooltip_message),
            onDismiss = onTooltipDismiss,
        )
    }
}

@Composable
fun rememberScannerIconColor(animating: Boolean): Color {
    val restingColor = PolkadotTheme.colors.fg.secondary
    val accentColor = PolkadotTheme.colors.fg.tertiary

    val phase = remember { Animatable(0f) }
    val idle = remember { Animatable(1f) }

    LaunchedEffect(animating) {
        if (animating) {
            launch { idle.animateTo(0f, tween(SCANNER_ANIMATION_DURATION_MS, easing = LinearEasing)) }
            phase.animateTo(
                targetValue = 1f,
                animationSpec = infiniteRepeatable(
                    animation = tween(durationMillis = SCANNER_ANIMATION_DURATION_MS, easing = LinearEasing),
                    repeatMode = RepeatMode.Reverse
                )
            )
        } else {
            idle.animateTo(1f, tween(SCANNER_ANIMATION_DURATION_MS, easing = LinearEasing))
        }
    }

    return lerp(restingColor, accentColor, phase.value * (1f - idle.value))
}

private const val SCANNER_ANIMATION_DURATION_MS = 800
