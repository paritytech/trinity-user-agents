package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.toRect
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.toSize
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrush
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrushTip
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskMode
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShapes
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import kotlin.math.roundToInt

/**
 * Masks the node: the tree's shapes, those the worker added since, and the followed pan's strokes, each read in the
 * draw phase only, so an added shape or a new touch redraws the node, composes nothing, and lands in the frame whose
 * touch caused it. During a fade into a new tree, the picture before the tree is blended away on the CSS `ease` curve.
 * A mask with a cleared share sends its action once the shapes cover enough of the node, as [MaskShareWatch] decides.
 * The node can be anything that draws: an image, a box of children, or a surface drawn by another renderer inside it.
 */
@Composable
internal fun JsModifier.Mask.maskModifier(): Modifier {
    val mask = this
    val density = LocalDensity.current.density
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    var bounds by remember { mutableStateOf(Rect.Zero) }
    var frameNanos by remember { mutableLongStateOf(0L) }
    val channel = mask.channel
    val coordinates = remember(mask.trail) { MaskCoordinates() }

    /** What the node draws now, in hundredths of a dp from its top-start corner. */
    fun picture(): JsMaskShapes.Picture {
        val followed = mask.follow?.let { follow ->
            val trail = mask.trail ?: return@let emptyList()

            // A press changes the coordinate transform even when the finger is stationary.
            @Suppress("UNUSED_VARIABLE", "unused")
            val drawnScale = trail.drawnScale
            val projection = coordinates.projection
            val strokes = if (projection == null) trail.strokes else trail.strokes.map { stroke -> stroke.map(projection::point) }
            val place = if (projection == null) trail.origin - bounds.topLeft else Offset.Zero
            MaskRules.followed(strokes, follow.width, place, density, if (rtl) bounds.width else null, follow.brush)
        }.orEmpty()
        return JsMaskShapes.Picture(mask.mode, mask.shapes + channel?.added.orEmpty() + followed)
    }

    SideEffect { channel?.source = ::picture }

    val share = mask.clearedShare
    if (mask.haptics != null || share?.whileDown == true || share?.rescue != null) {
        MaskProgress(mask, bounds, density) { picture() }
    } else if (share != null) {
        val dispatch by rememberUpdatedState(LocalJsEventHandler.current)
        val pictureNow by rememberUpdatedState { picture() }
        val trail = mask.trail
        val follows = mask.follow != null
        LaunchedEffect(share, channel, trail, follows) {
            fun input() = MaskShareWatch.Input(
                down = trail?.down ?: false,
                lifts = trail?.lifts ?: 0,
                trees = channel?.trees ?: 0,
                additions = channel?.additions ?: 0,
            )
            fun reached(): Boolean {
                val (covered, total) = MaskCoverage.count(
                    pictureNow().shapes,
                    width = (bounds.width / density * HUNDREDTHS).roundToInt(),
                    height = (bounds.height / density * HUNDREDTHS).roundToInt(),
                )
                return share.reached(covered, total)
            }
            val watch = MaskShareWatch(input())
            snapshotFlow { input() }.collect { input ->
                if (watch.update(input, follows, reached = ::reached)) dispatch(share.action, JsUiEvent.Type.ButtonClick)
            }
        }
    }

    val fade = channel?.fade
    if (fade != null) {
        LaunchedEffect(fade) {
            while (channel.fade === fade) {
                withFrameNanos { frameNanos = it }
                channel.endFade()
            }
        }
    }

    return Modifier
        .onGloballyPositioned {
            bounds = Rect(it.positionInRoot(), it.size.toSize())
            coordinates.node = it
            coordinates.update(mask.trail?.coordinates)
        }
        // Offscreen, so the mask reaches through the node only and never into what is drawn behind it.
        .graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen)
        .drawWithContent {
            coordinates.update(mask.trail?.coordinates)
            drawContent()
            val now = channel?.fade
            val current = picture()
            if (now == null && current.mode == JsMaskMode.HIDE) {
                drawShapes(current.shapes, density, rtl, BlendMode.DstOut)
                return@drawWithContent
            }
            drawIntoCanvas { canvas ->
                val rect = size.toRect()
                canvas.saveLayer(rect, Paint().apply { blendMode = BlendMode.DstIn })
                if (now == null) {
                    drawVisibility(current, density, rtl, 1f, BlendMode.SrcOver)
                } else {
                    // The frame's own time, read here so each frame of the fade draws again.
                    val progress = MaskRules.progress(maxOf(frameNanos, now.startNanos), now.startNanos, now.durationNanos)
                    drawVisibility(now.from, density, rtl, 1f - progress, BlendMode.SrcOver)
                    drawVisibility(current, density, rtl, progress, BlendMode.Plus)
                }
                canvas.restore()
            }
        }
}

/** Where the node shows, opaque, in a layer of its own composited with [alpha] and [blendMode]. */
private fun DrawScope.drawVisibility(
    picture: JsMaskShapes.Picture,
    density: Float,
    rtl: Boolean,
    alpha: Float,
    blendMode: BlendMode,
) = drawIntoCanvas { canvas ->
    canvas.saveLayer(size.toRect(), Paint().apply { this.alpha = alpha; this.blendMode = blendMode })
    if (picture.mode == JsMaskMode.HIDE) {
        drawRect(Color.Black)
        drawShapes(picture.shapes, density, rtl, BlendMode.DstOut)
    } else {
        drawShapes(picture.shapes, density, rtl, BlendMode.SrcOver)
    }
    canvas.restore()
}

private fun DrawScope.drawShapes(shapes: List<JsMaskShape>, density: Float, rtl: Boolean, blendMode: BlendMode) {
    if (shapes.isEmpty()) return
    val width = if (rtl) size.width else null
    val fills = MaskRules.fills(shapes, size, density, width)
    if (!fills.isEmpty) drawPath(fills, Color.Black, blendMode = blendMode)
    for ((stroke, path) in MaskRules.lines(shapes, density, width)) {
        drawPath(path, Color.Black, style = Stroke(stroke, cap = StrokeCap.Round, join = StrokeJoin.Round), blendMode = blendMode)
    }
}

/** A mask's geometry, kept apart from drawing so it can be checked on its own. */
internal object MaskRules {
    /** The CSS `ease` curve a fade follows. */
    val ease = CubicBezierEasing(0.25f, 0.1f, 0.25f, 1f)

    /** How far a fade is at [now], eased, 0 through 1. */
    fun progress(now: Long, start: Long, duration: Long): Float {
        if (duration <= 0L) return 1f
        return ease.transform(((now - start).toFloat() / duration).coerceIn(0f, 1f))
    }

    /** A wire place in px from the node's left edge; right to left ([mirroredWidth] given) it counts from the right. */
    fun px(point: JsMaskPoint, density: Float, mirroredWidth: Float?): Offset {
        val x = point.x * density / HUNDREDTHS
        return Offset(mirroredWidth?.let { it - x } ?: x, point.y * density / HUNDREDTHS)
    }

    /** The whole node, circles and one-point strokes (as dots) as one filled path. */
    fun fills(shapes: List<JsMaskShape>, size: Size, density: Float, mirroredWidth: Float?): Path {
        val path = Path()
        for (shape in shapes) {
            when (shape) {
                JsMaskShape.All -> path.addRect(size.toRect())
                is JsMaskShape.Stamped -> path.addPath(stamps(shape, density, mirroredWidth))
                is JsMaskShape.Circle -> if (shape.radius > 0) {
                    path.addOval(circle(px(shape.center, density, mirroredWidth), shape.radius * density / HUNDREDTHS))
                }
                is JsMaskShape.Stroke -> if (shape.points.size == 1 && shape.width > 0) {
                    path.addOval(circle(px(shape.points[0], density, mirroredWidth), shape.width * density / HUNDREDTHS / 2))
                }
            }
        }
        return path
    }

    private fun stamps(shape: JsMaskShape.Stamped, density: Float, mirroredWidth: Float?): Path {
        val path = Path()
        val halfLength = shape.width * density / HUNDREDTHS / 2
        val halfThickness = halfLength * shape.brush.aspectPermille.coerceIn(1, 1000) / 1000f
        val rect = Rect(-halfLength, -halfThickness, halfLength, halfThickness)
        val angle = shape.brush.angleCentideg / 100f * if (mirroredWidth == null) 1 else -1
        for (centre in MaskStamps.centres(shape.width, shape.brush, shape.points)) {
            val tip = Path().apply {
                when (shape.brush.tip) {
                    JsMaskBrushTip.ELLIPSE -> addOval(rect)
                    JsMaskBrushTip.CAPSULE -> addRoundRect(RoundRect(rect, CornerRadius(halfThickness)))
                }
            }
            val x = (centre.x * density / HUNDREDTHS).toFloat()
            val y = (centre.y * density / HUNDREDTHS).toFloat()
            tip.transform(Matrix().apply {
                translate(mirroredWidth?.let { it - x } ?: x, y)
                rotateZ(angle)
            })
            path.addPath(tip)
        }
        return path
    }

    /** Strokes of two points or more, one path per width in px, in the order the widths first appear. */
    fun lines(shapes: List<JsMaskShape>, density: Float, mirroredWidth: Float?): List<Pair<Float, Path>> {
        val paths = LinkedHashMap<Long, Path>()
        for (shape in shapes) {
            if (shape !is JsMaskShape.Stroke || shape.points.size < 2 || shape.width <= 0) continue
            val path = paths.getOrPut(shape.width) { Path() }
            val first = px(shape.points[0], density, mirroredWidth)
            path.moveTo(first.x, first.y)
            for (point in shape.points.drop(1)) {
                val at = px(point, density, mirroredWidth)
                path.lineTo(at.x, at.y)
            }
        }
        return paths.map { (width, path) -> width * density / HUNDREDTHS to path }
    }

    /**
     * The followed pan's strokes (px from the pan's top-left corner) as wire strokes [width] wide: moved by [place]
     * (the pan's corner in the node), and counted from the right edge of a right-to-left node [mirroredWidth] wide.
     */
    fun followed(
        strokes: List<List<Offset>>,
        width: Long,
        place: Offset,
        density: Float,
        mirroredWidth: Float?,
        brush: JsMaskBrush? = null,
    ): List<JsMaskShape> = strokes.filter { it.isNotEmpty() }.map { stroke ->
        val points = stroke.map { point ->
            val x = point.x + place.x
            JsMaskPoint(
                x = ((mirroredWidth?.let { it - x } ?: x) / density * HUNDREDTHS).roundToInt(),
                y = ((point.y + place.y) / density * HUNDREDTHS).roundToInt(),
            )
        }
        if (brush == null) JsMaskShape.Stroke(width, points) else JsMaskShape.Stamped(width, brush, points)
    }

    private fun circle(center: Offset, radius: Float) =
        Rect(center.x - radius, center.y - radius, center.x + radius, center.y + radius)
}

private const val HUNDREDTHS = 100f
