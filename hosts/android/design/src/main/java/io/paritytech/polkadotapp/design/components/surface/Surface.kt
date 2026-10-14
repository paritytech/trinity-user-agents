package io.paritytech.polkadotapp.design.components.surface

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.material3.LocalAbsoluteTonalElevation
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Stable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun PolkadotSurface(
    modifier: Modifier = Modifier,
    shape: Shape = RectangleShape,
    color: Color,
    contentColor: Color = PolkadotTheme.colors.fg.primary,
    border: BorderStroke? = null,
    tonalElevation: Dp = 0.dp,
    shadowElevation: Dp = 0.dp,
    contentAlignment: Alignment = Alignment.TopStart,
    enabled: Boolean = true,
    interactionSource: MutableInteractionSource? = null,
    onClick: (() -> Unit)? = null,
    content: @Composable BoxScope.() -> Unit,
) {
    PolkadotSurface(
        modifier = modifier,
        shape = shape,
        brush = SolidColor(color),
        contentColor = contentColor,
        border = border,
        tonalElevation = tonalElevation,
        shadowElevation = shadowElevation,
        contentAlignment = contentAlignment,
        enabled = enabled,
        interactionSource = interactionSource,
        onClick = onClick,
        content = content,
    )
}

@Composable
fun PolkadotSurface(
    modifier: Modifier = Modifier,
    shape: Shape = RectangleShape,
    brush: Brush = SolidColor(PolkadotTheme.colors.bg.surface.main),
    contentColor: Color = PolkadotTheme.colors.fg.primary,
    border: BorderStroke? = null,
    tonalElevation: Dp = 0.dp,
    shadowElevation: Dp = 0.dp,
    contentAlignment: Alignment = Alignment.TopStart,
    enabled: Boolean = true,
    interactionSource: MutableInteractionSource? = null,
    onClick: (() -> Unit)? = null,
    content: @Composable BoxScope.() -> Unit,
) {
    NovaSurfaceInternal(
        modifier = modifier,
        shape = shape,
        brush = brush,
        contentColor = contentColor,
        border = border,
        tonalElevation = tonalElevation,
        shadowElevation = shadowElevation,
        contentAlignment = contentAlignment,
        onClick = onClick,
        enabled = enabled,
        interactionSource = interactionSource,
        content = content,
    )
}

@Composable
private fun NovaSurfaceInternal(
    modifier: Modifier,
    shape: Shape,
    brush: Brush,
    contentColor: Color,
    tonalElevation: Dp,
    shadowElevation: Dp,
    border: BorderStroke?,
    contentAlignment: Alignment,
    enabled: Boolean,
    onClick: (() -> Unit)?,
    interactionSource: MutableInteractionSource?,
    content: @Composable BoxScope.() -> Unit,
) {
    val absoluteElevation = LocalAbsoluteTonalElevation.current + tonalElevation

    CompositionLocalProvider(
        LocalContentColor provides contentColor,
        LocalAbsoluteTonalElevation provides absoluteElevation
    ) {
        val clickableModifier = if (onClick != null) {
            Modifier.clickable(
                interactionSource = interactionSource,
                indication = LocalIndication.current,
                enabled = enabled,
                onClick = onClick
            )
        } else Modifier

        Box(
            modifier = modifier
                .surface(
                    shape = shape,
                    background = brush,
                    border = border,
                    shadowElevation = with(LocalDensity.current) { shadowElevation.toPx() }
                )
                .then(clickableModifier),
            propagateMinConstraints = true,
            contentAlignment = contentAlignment,
            content = content
        )
    }
}

@Stable
private fun Modifier.surface(
    shape: Shape,
    background: Brush,
    border: BorderStroke?,
    shadowElevation: Float,
) =
    this
        .then(
            if (shadowElevation > 0f) {
                Modifier.graphicsLayer(
                    shadowElevation = shadowElevation,
                    shape = shape,
                    clip = false
                )
            } else {
                Modifier
            }
        )
        .then(if (border != null) Modifier.border(border, shape) else Modifier)
        .background(brush = background, shape = shape)
        .clip(shape)
