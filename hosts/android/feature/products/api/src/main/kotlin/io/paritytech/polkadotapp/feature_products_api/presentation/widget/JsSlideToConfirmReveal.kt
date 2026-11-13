package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRightShaft
import io.paritytech.polkadotapp.design.components.icon.vectors.Check
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlin.math.abs
import kotlin.math.min

@Composable
internal fun SlideToConfirmRevealFace(
    widget: JsWidget.SlideToConfirm,
    progress: () -> Float,
    modifier: Modifier = Modifier,
) {
    val colors = PolkadotTheme.colors
    val accent = widget.accent?.toComposeColor() ?: colors.fg.warning
    val onAccent = contrastingOn(accent, colors.fg.primary, colors.fg.primaryInverted)
    val accentDeep = lerp(accent, onAccent, ACCENT_DEPTH)
    val accentFill = remember(accent, accentDeep) { Brush.verticalGradient(listOf(accent, accentDeep)) }
    val faceColor = colors.bg.surface.nested
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    val cornerRadius = PolkadotTheme.radii.large
    val outlineCorner = cornerRadius - OUTLINE_INSET
    val faceContentPadding = PolkadotTheme.spacings.mediumIncreased
    val faceUncovered = !widget.loading && !widget.confirmed

    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(CARD_HEIGHT)
            .graphicsLayer {
                shadowElevation = GLOW_ELEVATION.toPx()
                shape = RoundedCornerShape(cornerRadius)
                clip = true
                ambientShadowColor = accent
                spotShadowColor = accent
            }
            .background(accentFill)
            .dashedOutline(accentDeep, OUTLINE_INSET, outlineCorner),
        contentAlignment = Alignment.CenterStart,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = faceContentPadding + BORDER_WIDTH),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = widget.confirmedText ?: widget.text,
                style = PolkadotTheme.typography.title.smallEmphasized,
                color = onAccent,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Box(modifier = Modifier.size(BUTTON_SIZE), contentAlignment = Alignment.Center) {
                when {
                    widget.loading -> NovaCircularProgressIndicator(
                        modifier = Modifier.size(ICON_SIZE),
                        color = onAccent,
                        strokeWidth = SPINNER_STROKE,
                        trackColor = Color.Transparent,
                    )
                    widget.confirmed -> NovaIcon(
                        imageVector = NovaIcons.Check,
                        modifier = Modifier.size(ICON_SIZE),
                        tint = onAccent,
                    )
                }
            }
        }

        if (faceUncovered) {
            Box(
                modifier = Modifier
                    .matchParentSize()
                    .peeledAway(progress, rtl)
                    .padding(BORDER_WIDTH)
                    .background(faceColor, RoundedCornerShape(cornerRadius - BORDER_WIDTH))
                    .dashedOutline(colors.fg.tertiary.copy(alpha = FACE_OUTLINE_ALPHA), OUTLINE_INSET - BORDER_WIDTH, outlineCorner),
                contentAlignment = Alignment.CenterStart,
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = faceContentPadding),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = widget.text,
                            style = PolkadotTheme.typography.body.mediumEmphasized,
                            color = colors.fg.tertiary,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        widget.secondaryText?.let { secondary ->
                            Text(
                                text = secondary,
                                style = PolkadotTheme.typography.title.smallEmphasized,
                                color = colors.fg.tertiary,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    Box(
                        modifier = Modifier
                            .size(BUTTON_SIZE)
                            .background(accentFill, CircleShape),
                        contentAlignment = Alignment.Center,
                    ) {
                        NovaIcon(
                            imageVector = NovaIcons.ArrowRightShaft,
                            modifier = Modifier
                                .size(ICON_SIZE)
                                .graphicsLayer { scaleX = if (rtl) -1f else 1f },
                            tint = onAccent,
                        )
                    }
                }
            }
            Spacer(
                modifier = Modifier
                    .matchParentSize()
                    .peelRoll(
                        progress = progress,
                        rtl = rtl,
                        body = lerp(faceColor, colors.fg.primary, ROLL_BODY_LIFT),
                        sheen = lerp(faceColor, colors.fg.primary, ROLL_SHEEN_LIFT),
                        castShade = onAccent.copy(alpha = ROLL_SHADE_ALPHA),
                        cornerRadius = cornerRadius - BORDER_WIDTH,
                    ),
            )
        }
    }
}

private fun contrastingOn(background: Color, first: Color, second: Color): Color {
    val backgroundLuminance = background.luminance()
    return if (abs(first.luminance() - backgroundLuminance) >= abs(second.luminance() - backgroundLuminance)) first else second
}

private fun DrawScope.peelStart(progress: Float): Float {
    val inset = BORDER_WIDTH.toPx()
    return inset + progress.coerceIn(0f, 1f) * (size.width - 2 * inset)
}

private fun DrawScope.rollEnd(progress: Float): Float {
    val p = progress.coerceIn(0f, 1f)
    val inset = BORDER_WIDTH.toPx()
    val minWidth = ROLL_MIN_WIDTH.toPx()
    val gathered = minWidth + ((size.width - 2 * inset) * ROLL_MAX_FRACTION - minWidth) * p
    val emerged = (p / ROLL_EMERGE_PROGRESS).coerceIn(0f, 1f)
    return min(peelStart(p) + gathered * emerged, size.width - inset)
}

private fun Modifier.peeledAway(progress: () -> Float, rtl: Boolean): Modifier = drawWithContent {
    val edge = rollEnd(progress())
    clipRect(
        left = if (rtl) 0f else edge,
        right = if (rtl) size.width - edge else size.width,
    ) {
        this@drawWithContent.drawContent()
    }
}

private fun Modifier.peelRoll(
    progress: () -> Float,
    rtl: Boolean,
    body: Color,
    sheen: Color,
    castShade: Color,
    cornerRadius: Dp,
): Modifier = drawWithCache {
    val inset = BORDER_WIDTH.toPx()
    val shadeWidth = ROLL_SHADE_WIDTH.toPx()
    val maxCorner = cornerRadius.toPx()
    val rollFill = Brush.horizontalGradient(listOf(sheen, body), startX = 0f, endX = ROLL_SHEEN_WIDTH.toPx())
    val shadeFill = Brush.horizontalGradient(listOf(Color.Transparent, castShade), startX = -shadeWidth, endX = 0f)
    onDrawBehind {
        val p = progress()
        val start = peelStart(p)
        val width = rollEnd(p) - start
        if (width <= 0.5f) return@onDrawBehind
        val height = size.height - 2 * inset
        withTransform({
            if (rtl) scale(scaleX = -1f, scaleY = 1f, pivot = center)
            translate(left = start)
        }) {
            drawRect(shadeFill, topLeft = Offset(-shadeWidth, inset), size = Size(shadeWidth, height))
            drawRoundRect(
                brush = rollFill,
                topLeft = Offset(0f, inset),
                size = Size(width, height),
                cornerRadius = CornerRadius(min(maxCorner, width / 2)),
            )
        }
    }
}

private fun Modifier.dashedOutline(color: Color, inset: Dp, cornerRadius: Dp): Modifier = drawWithCache {
    val insetPx = inset.toPx()
    val stroke = Stroke(
        width = OUTLINE_WIDTH.toPx(),
        pathEffect = PathEffect.dashPathEffect(floatArrayOf(OUTLINE_DASH.toPx(), OUTLINE_GAP.toPx())),
    )
    val outlineSize = Size(size.width - 2 * insetPx, size.height - 2 * insetPx)
    val corner = CornerRadius(cornerRadius.toPx())
    onDrawBehind {
        drawRoundRect(color = color, topLeft = Offset(insetPx, insetPx), size = outlineSize, cornerRadius = corner, style = stroke)
    }
}

private val CARD_HEIGHT = 96.dp
private val BORDER_WIDTH = 2.dp
private val GLOW_ELEVATION = 12.dp
private val BUTTON_SIZE = 56.dp
private val ICON_SIZE = 24.dp
private val SPINNER_STROKE = 2.dp
private val OUTLINE_INSET = 8.dp
private val OUTLINE_WIDTH = 1.dp
private val OUTLINE_DASH = 4.dp
private val OUTLINE_GAP = 3.dp
private val ROLL_MIN_WIDTH = 28.dp
private val ROLL_SHEEN_WIDTH = 10.dp
private val ROLL_SHADE_WIDTH = 8.dp
private const val ROLL_MAX_FRACTION = 1f / 3f
private const val ROLL_EMERGE_PROGRESS = 0.04f
private const val ROLL_BODY_LIFT = 0.06f
private const val ROLL_SHEEN_LIFT = 0.3f
private const val ROLL_SHADE_ALPHA = 0.35f
private const val ACCENT_DEPTH = 0.3f
private const val FACE_OUTLINE_ALPHA = 0.5f

