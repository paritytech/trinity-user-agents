package io.paritytech.polkadotapp.design.components.tooltip

import androidx.compose.animation.*
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.coerceAtLeast
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.flow.drop

private val transitionAnimation = tween<Float>(durationMillis = 250)
private val TooltipShadowElevation = 2.dp

@Composable
internal fun TooltipAnimatedContent(
    expandedStates: MutableTransitionState<Boolean>,
    onCompletelyGone: () -> Unit,
    onCompletelyVisible: () -> Unit,
    transformOriginState: TransformOrigin,
    arrowCenterOffset: IntOffset,
    arrowVisible: Boolean,
    alignment: TooltipAlignment,
    backgroundColor: Color,
    shape: Shape,
    content: @Composable () -> Unit,
) {
    LaunchedEffect(Unit) {
        snapshotFlow { expandedStates.currentState }
            .drop(1)
            .collect { visible ->
                if (visible) {
                    onCompletelyVisible()
                } else {
                    onCompletelyGone()
                }
            }
    }

    AnimatedVisibility(
        visibleState = expandedStates,
        enter = fadeIn(transitionAnimation) + scaleIn(transitionAnimation, transformOrigin = transformOriginState),
        exit = fadeOut(transitionAnimation) + scaleOut(transitionAnimation, transformOrigin = transformOriginState),
    ) {
        ContentWithArrow(
            alignment = alignment,
            arrow = if (arrowVisible) {
                {
                    val arrowSize = getArrowSize(alignment)

                    ArrowWidthPaddings(
                        centerOffset = arrowCenterOffset,
                        size = arrowSize,
                        tooltipAlignment = alignment,
                        tint = backgroundColor
                    )
                }
            } else {
                null
            },
            content = {
                PolkadotSurface(
                    color = backgroundColor,
                    shape = shape,
                    shadowElevation = TooltipShadowElevation,
                ) {
                    content()
                }
            }
        )
    }
}

@Composable
private fun ContentWithArrow(
    modifier: Modifier = Modifier,
    alignment: TooltipAlignment,
    arrow: (@Composable () -> Unit)?,
    content: @Composable () -> Unit
) {
    val screenPadding = PolkadotTheme.spacings.small
    val paddingModifier = modifier
        .padding(getContentPaddingValues(alignment, screenPadding))

    if (arrow == null) {
        Box(
            modifier = paddingModifier
        ) {
            content()
        }
    } else {
        when (alignment) {
            TooltipAlignment.Bottom -> {
                Column(
                    modifier = paddingModifier
                ) {
                    arrow()
                    content()
                }
            }
            TooltipAlignment.Top -> {
                Column(
                    modifier = paddingModifier
                ) {
                    content()
                    arrow()
                }
            }
            TooltipAlignment.End -> {
                Row(
                    modifier = paddingModifier
                ) {
                    arrow()
                    content()
                }
            }
            TooltipAlignment.Start -> {
                Row(
                    modifier = paddingModifier
                ) {
                    content()
                    arrow()
                }
            }
        }
    }
}

@Composable
private fun ArrowWidthPaddings(
    centerOffset: IntOffset,
    size: DpSize,
    tooltipAlignment: TooltipAlignment,
    tint: Color,
) {
    val screenPadding = PolkadotTheme.spacings.small
    TooltipArrow(
        modifier = Modifier
            .padding(
                start = with(LocalDensity.current) {
                    (centerOffset.x.toDp() - size.width / 2 - screenPadding)
                        .coerceAtLeast(0.dp)
                },
                top = with(LocalDensity.current) {
                    (centerOffset.y.toDp() - size.height / 2 - screenPadding)
                        .coerceAtLeast(0.dp)
                }
            ),
        tooltipAlignment = tooltipAlignment,
        tint = tint,
    )
}

private fun getContentPaddingValues(alignment: TooltipAlignment, screenPadding: androidx.compose.ui.unit.Dp): PaddingValues =
    when (alignment) {
        TooltipAlignment.Top -> PaddingValues(
            top = screenPadding,
            start = screenPadding,
            end = screenPadding,
        )
        TooltipAlignment.Bottom -> PaddingValues(
            start = screenPadding,
            end = screenPadding,
            bottom = screenPadding,
        )
        TooltipAlignment.Start -> PaddingValues(
            start = screenPadding,
            top = screenPadding,
            bottom = screenPadding,
        )
        TooltipAlignment.End -> PaddingValues(
            end = screenPadding,
            top = screenPadding,
            bottom = screenPadding,
        )
    }

@Composable
private fun getArrowSize(alignment: TooltipAlignment): DpSize =
    when (alignment) {
        TooltipAlignment.Top,
        TooltipAlignment.Bottom -> {
            DpSize(28.dp, 22.dp)
        }
        TooltipAlignment.Start,
        TooltipAlignment.End -> {
            DpSize(22.dp, 28.dp)
        }
    }
