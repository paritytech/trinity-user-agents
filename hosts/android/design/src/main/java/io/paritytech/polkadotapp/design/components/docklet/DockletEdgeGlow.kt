package io.paritytech.polkadotapp.design.components.docklet

import android.graphics.Bitmap
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
import androidx.compose.runtime.mutableFloatStateOf
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
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.launch
import java.nio.ByteBuffer
import kotlin.math.min
import kotlin.math.roundToInt

private val GlowWidth = DockletDrawbridge.GlowWidth.dp
private const val BoostAlphaScale = 1.9f
private const val BoostCopyOpacity = 0.6f
private const val BoostScale = 1.08f
private const val BoostDamping = 0.6f
private const val BoostInMillis = 220
private const val FadeOutMillis = 320
private const val ShimmerCycleMillis = 1400
private const val ShimmerOpacity = 0.95f
private const val MaxAlpha = 255
private const val PixelCenter = 0.5f

@Composable
fun DockletEdgeGlow(
    state: DockletHideState,
    grabZone: DockletFrame,
    masks: DockletGlowMasks,
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

    var fadeWidth by remember { mutableFloatStateOf(1f) }
    LaunchedEffect(hidden) {
        if (hidden) {
            fade.snapTo(1f)
        } else {
            fadeWidth = state.glowAlpha
            fade.snapTo(state.glowAlpha)
            fade.animateTo(0f, tween(FadeOutMillis, easing = DockletMotion.ContentsEasing))
        }
    }
    val composed by remember(state) { derivedStateOf { state.countsAsHidden || fade.value > 0f } }
    val glowProgress = remember(state, hidden) { { if (hidden) state.glowAlpha else fade.value } }
    val widthProgress = remember(state, hidden) { { if (hidden) state.glowAlpha else fadeWidth } }
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
                    alpha = glowProgress()
                    compositingStrategy = CompositingStrategy.Offscreen
                }
                .drawWithCache {
                    val bandWidth = GlowWidth.toPx()
                    val bandLeft = size.width - bandWidth
                    val bandMasks = masks.at(this, bandWidth, size.height)
                    val tint = ColorFilter.tint(color)
                    val rainbow = Brush.verticalGradient(DockletShimmerColors, startY = 0f, endY = size.height, tileMode = TileMode.Repeated)
                    val band = Rect(bandLeft, 0f, size.width, size.height)
                    val bandOrigin = Offset(bandLeft, 0f)
                    val shimmerLayer = Paint().apply { blendMode = BlendMode.Plus }
                    onDrawBehind {
                        val widthScale = DockletGlowProfile.widthScale(widthProgress()) * scale.value
                        scale(scaleX = widthScale, scaleY = 1f, pivot = Offset(size.width, size.height / 2)) {
                            drawImage(bandMasks.base, topLeft = bandOrigin, colorFilter = tint)
                            drawImage(bandMasks.boost, topLeft = bandOrigin, alpha = boost.value * BoostCopyOpacity, colorFilter = tint)
                            if (shimmer.value > 0f) {
                                drawContext.canvas.saveLayer(band, shimmerLayer)
                                translate(top = phase.value * size.height) {
                                    drawRect(
                                        brush = rainbow,
                                        topLeft = Offset(bandLeft, -size.height),
                                        size = Size(bandWidth, size.height * 2),
                                        alpha = shimmer.value * ShimmerOpacity,
                                    )
                                }
                                drawImage(bandMasks.shimmer, topLeft = bandOrigin, blendMode = BlendMode.DstIn)
                                drawContext.canvas.restore()
                            }
                        }
                    }
                },
        )
    }
}

internal class GlowMasks(val width: Float, val height: Float, val base: ImageBitmap, val boost: ImageBitmap, val shimmer: ImageBitmap)

class DockletGlowMasks internal constructor() {
    private var cached: GlowMasks? = null

    internal fun at(density: Density, bandWidthPx: Float, heightPx: Float): GlowMasks =
        cached?.takeIf { it.width == bandWidthPx && it.height == heightPx }
            ?: density.glowMasks(bandWidthPx, heightPx).also { cached = it }
}

@Composable
fun rememberDockletGlowMasks(): DockletGlowMasks = remember { DockletGlowMasks() }

private fun Density.glowMasks(bandWidthPx: Float, heightPx: Float): GlowMasks {
    val length = heightPx / density
    return GlowMasks(
        width = bandWidthPx,
        height = heightPx,
        base = alphaMask(bandWidthPx, heightPx) { inFromEdge, along -> DockletGlowProfile.glowAlpha(inFromEdge, along, length) },
        boost = alphaMask(bandWidthPx, heightPx) { inFromEdge, along ->
            min(1f, BoostAlphaScale * DockletGlowProfile.glowAlpha(inFromEdge, along, length))
        },
        shimmer = alphaMask(bandWidthPx, heightPx) { inFromEdge, along -> DockletGlowProfile.shimmerStrength(inFromEdge, along, length) },
    )
}

private inline fun Density.alphaMask(widthPx: Float, heightPx: Float, alpha: (inFromEdge: Float, along: Float) -> Float): ImageBitmap {
    val width = widthPx.roundToInt().coerceAtLeast(1)
    val height = heightPx.roundToInt().coerceAtLeast(1)
    val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ALPHA_8)
    val rowBytes = bitmap.rowBytes
    val pixels = ByteArray(rowBytes * height)
    for (row in 0 until height) {
        val along = (row + PixelCenter) / density
        for (column in 0 until width) {
            val inFromEdge = (width - column - PixelCenter) / density
            pixels[row * rowBytes + column] = (alpha(inFromEdge, along).coerceIn(0f, 1f) * MaxAlpha).roundToInt().toByte()
        }
    }
    bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(pixels))
    return bitmap.asImageBitmap()
}
