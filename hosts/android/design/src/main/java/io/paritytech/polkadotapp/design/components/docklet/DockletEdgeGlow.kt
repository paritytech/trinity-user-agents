package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.systemGestureExclusion
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.launch

private val GlowWidth = DockletDrawbridge.GlowWidth.dp
private val FeatherLength = 22.dp
private val ShimmerColumn = 4.dp
private const val ProfileStops = 13
private const val BoostAlphaScale = 1.9f
private const val BoostCopyOpacity = 0.6f
private const val BoostScale = 1.08f
private const val BoostDamping = 0.6f
private const val BoostInMillis = 220
private const val FadeOutMillis = 320
private const val ShimmerCycleMillis = 1400
private const val ShimmerOpacity = 0.95f

@Composable
fun DockletEdgeGlow(
    state: DockletHideState,
    grabZone: DockletFrame,
    label: String,
) {
    var held by remember { mutableStateOf(false) }
    val fade = remember { Animatable(0f) }
    val boost = remember { Animatable(0f) }
    val scale = remember { Animatable(1f) }
    val shimmer = remember { Animatable(0f) }
    val phase = remember { Animatable(0f) }
    val hidden = state.countsAsHidden
    val boosted = held || state.isHeld
    val glowWidthPx = with(LocalDensity.current) { GlowWidth.toPx() }

    LaunchedEffect(hidden) {
        if (hidden) {
            fade.snapTo(1f)
        } else {
            fade.snapTo(state.glowAlpha)
            fade.animateTo(0f, tween(FadeOutMillis, easing = DockletMotion.ContentsEasing))
        }
    }
    val composed by remember(state) { derivedStateOf { state.countsAsHidden || fade.value > 0f } }
    LaunchedEffect(boosted) {
        launch { boost.animateTo(if (boosted) 1f else 0f, tween(if (boosted) BoostInMillis else FadeOutMillis)) }
        launch { scale.animateTo(if (boosted) BoostScale else 1f, spring(dampingRatio = BoostDamping)) }
        launch { shimmer.animateTo(if (boosted) 1f else 0f, tween(if (boosted) BoostInMillis else FadeOutMillis)) }
        if (boosted) {
            phase.animateTo(1f, infiniteRepeatable(tween(ShimmerCycleMillis, easing = LinearEasing), RepeatMode.Restart))
        }
    }

    if (composed) {
        val color = PolkadotTheme.colors.bg.surface.containerInverted
        Box(
            modifier = Modifier
                .offset(grabZone.left, grabZone.top)
                .size(grabZone.width, grabZone.height)
                .then(
                    if (hidden) {
                        Modifier.systemGestureExclusion { Rect(it.size.width - glowWidthPx, 0f, it.size.width.toFloat(), it.size.height.toFloat()) }
                    } else {
                        Modifier
                    },
                )
                .semantics {
                    contentDescription = label
                    onClick(label) { state.restore(); true }
                }
                .then(rememberDockletRestoreDrag(state) { held = it })
                .graphicsLayer {
                    alpha = if (hidden) state.glowAlpha else fade.value
                    compositingStrategy = CompositingStrategy.Offscreen
                }
                .drawWithCache {
                    val bandWidth = GlowWidth.toPx()
                    val bandLeft = size.width - bandWidth
                    val band = profileBrush(color, bandLeft, size.width, alphaScale = 1f)
                    val boosted = profileBrush(color, bandLeft, size.width, alphaScale = BoostAlphaScale)
                    val feather = FeatherLength.toPx().coerceAtMost(size.height / 2) / size.height
                    val featherMask = Brush.verticalGradient(
                        0f to Color.Transparent,
                        feather to Color.Black,
                        1f - feather to Color.Black,
                        1f to Color.Transparent,
                    )
                    val rainbow = Brush.verticalGradient(DockletShimmerColors, startY = 0f, endY = size.height, tileMode = TileMode.Repeated)
                    val column = ShimmerColumn.toPx()
                    val columns = (bandWidth / column).toInt()
                    val bandSize = Size(bandWidth, size.height)
                    onDrawBehind {
                        scale(scaleX = scale.value, scaleY = 1f, pivot = Offset(size.width, size.height / 2)) {
                            drawRect(band, topLeft = Offset(bandLeft, 0f), size = bandSize)
                            drawRect(boosted, topLeft = Offset(bandLeft, 0f), size = bandSize, alpha = boost.value * BoostCopyOpacity)
                            if (shimmer.value > 0f) {
                                translate(top = phase.value * size.height) {
                                    repeat(columns) { index ->
                                        val strength = (index + 1f) / columns
                                        drawRect(
                                            brush = rainbow,
                                            topLeft = Offset(size.width - (columns - index) * column, -size.height),
                                            size = Size(column, size.height * 2),
                                            alpha = shimmer.value * ShimmerOpacity * strength,
                                            blendMode = BlendMode.Plus,
                                        )
                                    }
                                }
                            }
                        }
                        drawRect(featherMask, blendMode = BlendMode.DstIn)
                    }
                },
        )
    }
}

private fun profileBrush(color: Color, left: Float, right: Float, alphaScale: Float): Brush {
    val stops = Array(ProfileStops) { index ->
        val fraction = index / (ProfileStops - 1f)
        val inFromEdge = DockletDrawbridge.GlowWidth * (1f - fraction)
        fraction to color.copy(alpha = (DockletDrawbridge.glowAlpha(inFromEdge) * alphaScale).coerceIn(0f, 1f))
    }
    return Brush.horizontalGradient(*stops, startX = left, endX = right)
}
