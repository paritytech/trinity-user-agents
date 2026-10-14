package io.paritytech.polkadotapp.app.root.presentation.main.compose.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.tooltip.NonFocusablePopupProperties
import io.paritytech.polkadotapp.design.components.tooltip.PolkadotTooltip
import io.paritytech.polkadotapp.design.components.tooltip.PolkadotTooltipContent
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.launch
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * The scanner's animated icon plus its first-run tooltip, with no surface of its own — the caller supplies
 * the container and the click (in the tab bar, the shared center pill).
 */
@Composable
fun ScannerIconWithTooltip(
    modifier: Modifier = Modifier,
    tooltipVisible: Boolean,
    active: Boolean,
    onTooltipDismiss: () -> Unit,
) {
    Box(
        modifier = modifier,
        contentAlignment = Alignment.Center
    ) {
        AnimatedScannerIcon(animating = tooltipVisible, active = active)

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
}

@Composable
private fun AnimatedScannerIcon(animating: Boolean, active: Boolean) {
    val restingColor by animateColorAsState(
        targetValue = if (active) PolkadotTheme.colors.fg.primary else PolkadotTheme.colors.fg.secondary,
        label = "ScannerIconColor",
    )
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

    val cornersPath = remember { PathParser().parsePathString(CORNERS_PATH).toPath() }
    val crossPath = remember { PathParser().parsePathString(CROSS_PATH).toPath() }

    val active = 1f - idle.value
    val cornersColor = lerp(restingColor, accentColor, phase.value * active)
    val crossColor = lerp(restingColor, accentColor, (1f - phase.value) * active)

    Canvas(Modifier.size(24.dp)) {
        val factor = size.minDimension / SCANNER_VIEWPORT
        scale(factor, factor, pivot = Offset.Zero) {
            drawPath(cornersPath, cornersColor)
            drawPath(crossPath, crossColor)
        }
    }
}

private const val SCANNER_ANIMATION_DURATION_MS = 800
private const val SCANNER_VIEWPORT = 25f

private const val CORNERS_PATH =
    "M0 20.582 V18.176 C0 17.44 0.597 16.843 1.333 16.843 C2.069 16.843 2.666 17.44 2.666 18.176 " +
        "V20.582 C2.666 20.866 2.779 21.139 2.98 21.341 C3.182 21.542 3.455 21.655 3.739 21.655 " +
        "H6.145 C6.881 21.655 7.478 22.252 7.478 22.988 C7.478 23.724 6.881 24.321 6.145 24.321 " +
        "H3.739 C2.748 24.321 1.796 23.927 1.095 23.226 C0.394 22.524 0 21.573 0 20.582 Z " +
        "M21.655 20.582 V18.176 C21.655 17.44 22.252 16.843 22.988 16.843 C23.724 16.843 24.321 17.44 24.321 18.176 " +
        "V20.582 C24.321 21.573 23.927 22.524 23.226 23.226 C22.524 23.927 21.573 24.321 20.582 24.321 " +
        "H18.176 C17.44 24.321 16.843 23.724 16.843 22.988 C16.843 22.252 17.44 21.655 18.176 21.655 " +
        "H20.582 C20.866 21.655 21.139 21.542 21.341 21.341 C21.542 21.139 21.655 20.866 21.655 20.582 Z " +
        "M0 6.145 V3.739 C0 2.748 0.394 1.796 1.095 1.095 C1.796 0.394 2.748 0 3.739 0 " +
        "H6.145 C6.881 0 7.478 0.597 7.478 1.333 C7.478 2.069 6.881 2.666 6.145 2.666 " +
        "H3.739 C3.455 2.666 3.182 2.779 2.98 2.98 C2.779 3.182 2.666 3.455 2.666 3.739 " +
        "V6.145 C2.666 6.881 2.069 7.478 1.333 7.478 C0.597 7.478 0 6.881 0 6.145 Z " +
        "M21.655 6.145 V3.739 C21.655 3.455 21.542 3.182 21.341 2.98 C21.139 2.779 20.866 2.666 20.582 2.666 " +
        "H18.176 C17.44 2.666 16.843 2.069 16.843 1.333 C16.843 0.597 17.44 0 18.176 0 " +
        "H20.582 C21.573 0 22.524 0.394 23.226 1.095 C23.927 1.796 24.321 2.748 24.321 3.739 " +
        "V6.145 C24.321 6.881 23.724 7.478 22.988 7.478 C22.252 7.478 21.655 6.881 21.655 6.145 Z"

private const val CROSS_PATH =
    "M12.16 4.749 L12.16 4.749 A1.397 1.397 0 0 1 13.557 6.146 L13.557 17.982 " +
        "A1.397 1.397 0 0 1 12.16 19.379 L12.16 19.379 A1.397 1.397 0 0 1 10.764 17.982 " +
        "L10.764 6.146 A1.397 1.397 0 0 1 12.16 4.749 Z " +
        "M19.475 12.064 L19.475 12.064 A1.397 1.397 0 0 1 18.079 13.461 L6.242 13.461 " +
        "A1.397 1.397 0 0 1 4.846 12.064 L4.846 12.064 A1.397 1.397 0 0 1 6.242 10.667 " +
        "L18.079 10.667 A1.397 1.397 0 0 1 19.475 12.064 Z"
