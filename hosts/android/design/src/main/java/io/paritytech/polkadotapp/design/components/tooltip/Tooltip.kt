package io.paritytech.polkadotapp.design.components.tooltip

import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.runtime.*
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

internal val DefaultPopupProperties = PopupProperties(focusable = true)

val NonFocusablePopupProperties = PopupProperties(focusable = false)

@Composable
fun PolkadotTooltip(
    expanded: Boolean,
    onDismiss: () -> Unit,
    backgroundColor: Color = PolkadotTheme.colors.bg.surface.containerInverted,
    shape: Shape = PolkadotTheme.shapes.full,
    arrowVisible: Boolean,
    alignment: TooltipAlignment = TooltipAlignment.Top,
    properties: PopupProperties = DefaultPopupProperties,
    onCompletelyVisible: () -> Unit = {},
    onCompletelyGone: () -> Unit = {},
    content: @Composable () -> Unit,
) {
    val expandedStates = remember { MutableTransitionState(false) }
    expandedStates.targetState = expanded

    if (expandedStates.currentState || expandedStates.targetState) {
        val screenPaddingPx = with(LocalDensity.current) { PolkadotTheme.spacings.small.roundToPx() }
        var transformOriginState by remember { mutableStateOf(TransformOrigin.Center) }
        var arrowCenterOffset by remember { mutableStateOf(IntOffset.Zero) }

        val popupPositionProvider = remember {
            TooltipPositionProvider(
                screenPaddingPx = screenPaddingPx,
                tooltipAlignment = alignment,
                onLayout = { anchorBounds, tooltipBounds, arrowCenterIntOffset ->
                    transformOriginState = calculateTooltipTransformOrigin(anchorBounds, tooltipBounds)
                    arrowCenterOffset = arrowCenterIntOffset
                },
            )
        }

        Popup(
            onDismissRequest = onDismiss,
            popupPositionProvider = popupPositionProvider,
            properties = properties,
        ) {
            TooltipAnimatedContent(
                expandedStates = expandedStates,
                onCompletelyGone = onCompletelyGone,
                onCompletelyVisible = onCompletelyVisible,
                transformOriginState = transformOriginState,
                arrowCenterOffset = arrowCenterOffset,
                alignment = alignment,
                arrowVisible = arrowVisible,
                backgroundColor = backgroundColor,
                shape = shape,
                content = content,
            )
        }
    }
}
