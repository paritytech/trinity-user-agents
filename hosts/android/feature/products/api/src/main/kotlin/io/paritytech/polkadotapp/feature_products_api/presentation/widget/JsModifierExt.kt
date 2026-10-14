package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.toRect
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsBlendingMode
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsShape

@Composable
fun List<JsModifier>.toComposeModifier(): Modifier =
    orderedForCompose().fold(Modifier as Modifier) { acc, modifier ->
        acc.then(modifier.toComposeModifier())
    }

/**
 * Compose applies modifiers outside-in, and each one covers only what the ones after it draw.
 * Opacity and blending therefore have to come before the background and border, or a faded node
 * keeps an opaque background and a blended one never blends the surface it paints.
 *
 * The rest follows the frame: margin outside, then the surface, then the content's padding and size.
 */
internal fun List<JsModifier>.orderedForCompose(): List<JsModifier> = sortedWith(
    compareBy {
        when (it) {
            is JsModifier.Margin -> 0
            is JsModifier.Opacity -> 1
            is JsModifier.BlendingMode -> 2
            is JsModifier.Background -> 3
            is JsModifier.Border -> 4
            is JsModifier.Clip -> 5
            is JsModifier.Padding -> 6
            is JsModifier.Size -> 7
            is JsModifier.FillMaxWidth -> 8
            is JsModifier.FillMaxHeight -> 9
        }
    }
)

@Composable
fun JsModifier.toComposeModifier(): Modifier = when (this) {
    is JsModifier.Margin -> toMarginModifier()
    is JsModifier.Padding -> toPaddingModifier()
    is JsModifier.Background -> toBackgroundModifier()
    is JsModifier.Border -> toBorderModifier()
    is JsModifier.Size -> toSizeModifier()
    is JsModifier.FillMaxWidth -> Modifier.fillMaxWidth(fraction)
    is JsModifier.FillMaxHeight -> Modifier.fillMaxHeight(fraction)
    is JsModifier.Clip -> Modifier.clip(shape.toComposeShape())
    is JsModifier.Opacity -> Modifier.alpha(alpha / OPAQUE)
    is JsModifier.BlendingMode -> Modifier.blendedWith(mode.toComposeBlendMode())
}

private const val OPAQUE = 255f

// Compose has no blend-mode modifier; the node is drawn into its own layer composited with the mode.
private fun Modifier.blendedWith(blendMode: BlendMode): Modifier = drawWithContent {
    drawIntoCanvas { canvas ->
        canvas.saveLayer(size.toRect(), Paint().apply { this.blendMode = blendMode })
        drawContent()
        canvas.restore()
    }
}

private fun JsBlendingMode.toComposeBlendMode(): BlendMode = when (this) {
    JsBlendingMode.NORMAL -> BlendMode.SrcOver
    JsBlendingMode.MULTIPLY -> BlendMode.Multiply
    JsBlendingMode.SCREEN -> BlendMode.Screen
    JsBlendingMode.OVERLAY -> BlendMode.Overlay
    JsBlendingMode.DARKEN -> BlendMode.Darken
    JsBlendingMode.LIGHTEN -> BlendMode.Lighten
    JsBlendingMode.COLOR_DODGE -> BlendMode.ColorDodge
    JsBlendingMode.COLOR_BURN -> BlendMode.ColorBurn
    JsBlendingMode.HARD_LIGHT -> BlendMode.Hardlight
    JsBlendingMode.SOFT_LIGHT -> BlendMode.Softlight
    JsBlendingMode.DIFFERENCE -> BlendMode.Difference
    JsBlendingMode.EXCLUSION -> BlendMode.Exclusion
    JsBlendingMode.HUE -> BlendMode.Hue
    JsBlendingMode.SATURATION -> BlendMode.Saturation
    JsBlendingMode.COLOR -> BlendMode.Color
    JsBlendingMode.LUMINOSITY -> BlendMode.Luminosity
}

@Composable
private fun JsModifier.Margin.toMarginModifier(): Modifier {
    // Margin is converted to padding (applied before background in modifier chain)
    val allMargin = all
    val horizontalMargin = horizontal
    val verticalMargin = vertical
    return when {
        allMargin != null -> Modifier.padding(allMargin.dp)
        horizontalMargin != null || verticalMargin != null -> Modifier.padding(
            horizontal = (horizontalMargin ?: 0).dp,
            vertical = (verticalMargin ?: 0).dp
        )
        else -> Modifier.padding(
            start = (start ?: 0).dp,
            top = (top ?: 0).dp,
            end = (end ?: 0).dp,
            bottom = (bottom ?: 0).dp
        )
    }
}

@Composable
private fun JsModifier.Padding.toPaddingModifier(): Modifier {
    val allPadding = all
    val horizontalPadding = horizontal
    val verticalPadding = vertical
    return when {
        allPadding != null -> Modifier.padding(allPadding.dp)
        horizontalPadding != null || verticalPadding != null -> Modifier.padding(
            horizontal = (horizontalPadding ?: 0).dp,
            vertical = (verticalPadding ?: 0).dp
        )
        else -> Modifier.padding(
            start = (start ?: 0).dp,
            top = (top ?: 0).dp,
            end = (end ?: 0).dp,
            bottom = (bottom ?: 0).dp
        )
    }
}

@Composable
private fun JsModifier.Background.toBackgroundModifier(): Modifier {
    val bgColor = color.toComposeColor()
    val bgShape = shape
    return if (bgShape != null) {
        Modifier.background(bgColor, bgShape.toComposeShape())
    } else {
        Modifier.background(bgColor)
    }
}

@Composable
private fun JsModifier.Border.toBorderModifier(): Modifier {
    val borderColor = color.toComposeColor()
    val borderShape = shape?.toComposeShape() ?: PolkadotTheme.shapes.zero
    return Modifier.border(width.dp, borderColor, borderShape)
}

private fun JsModifier.Size.toSizeModifier(): Modifier {
    var modifier: Modifier = Modifier
    width?.let { modifier = modifier.width(it.dp) }
    height?.let { modifier = modifier.height(it.dp) }
    minWidth?.let { modifier = modifier.widthIn(min = it.dp) }
    maxWidth?.let { modifier = modifier.widthIn(max = it.dp) }
    minHeight?.let { modifier = modifier.heightIn(min = it.dp) }
    maxHeight?.let { modifier = modifier.heightIn(max = it.dp) }
    return modifier
}

@Composable
fun JsShape.toComposeShape(): Shape = when (this) {
    is JsShape.Circle -> PolkadotTheme.shapes.full
    is JsShape.Square -> RectangleShape
    is JsShape.Rounded -> {
        if (topStart != null || topEnd != null || bottomStart != null || bottomEnd != null) {
            RoundedCornerShape(
                topStart = (topStart ?: radius).dp,
                topEnd = (topEnd ?: radius).dp,
                bottomStart = (bottomStart ?: radius).dp,
                bottomEnd = (bottomEnd ?: radius).dp
            )
        } else {
            RoundedCornerShape(radius.dp)
        }
    }
}

@Composable
fun JsColor.toComposeColor(): Color = when (this) {
    JsColor.BG_SURFACE_MAIN -> PolkadotTheme.colors.bg.surface.main
    JsColor.BG_SURFACE_CONTAINER -> PolkadotTheme.colors.bg.surface.container
    JsColor.BG_SURFACE_NESTED -> PolkadotTheme.colors.bg.surface.nested
    JsColor.FG_PRIMARY -> PolkadotTheme.colors.fg.primary
    JsColor.FG_SECONDARY -> PolkadotTheme.colors.fg.secondary
    JsColor.FG_TERTIARY -> PolkadotTheme.colors.fg.tertiary
    JsColor.FG_ERROR -> PolkadotTheme.colors.fg.error
    JsColor.FG_SUCCESS -> PolkadotTheme.colors.fg.success
    JsColor.FG_WARNING -> PolkadotTheme.colors.fg.warning
    JsColor.FG_STATIC_WHITE -> PolkadotTheme.colors.fg.staticWhite
}
