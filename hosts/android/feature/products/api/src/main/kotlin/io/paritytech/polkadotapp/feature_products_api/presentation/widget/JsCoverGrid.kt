package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.graphics.ColorMatrixColorFilter
import android.graphics.RenderEffect
import android.graphics.Shader
import android.os.Build
import androidx.annotation.RequiresApi
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asComposeRenderEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import io.paritytech.polkadotapp.feature_products_api.model.JsCoverCells
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import java.util.BitSet
import kotlin.math.sqrt
import androidx.compose.ui.graphics.RenderEffect as ComposeRenderEffect

/**
 * Draws the children as a cover, with a round hole cut for every cleared cell: those of the tree, plus those the
 * worker cleared since (until the next tree, which always wins). The worker's cells are read in the draw phase only,
 * from a snapshot state written where the answer arrives, so a cleared-cells update redraws the cover, composes
 * nothing, and can be drawn in the frame whose touch sent the move ([JsCoverCells.drawn]); the brush's strokes are
 * read the same way ([JsPanTrail.drawn]). From Android 12 the holes' edge is smoothed as it is drawn
 * ([CoverGridRules.softening]), so cells read as one scratch, not as steps; before that it is drawn as is.
 * With a brush, the cells cleared since the tree are drawn as the brush's strokes along the finger instead, cut only
 * near those cells ([CoverGridRules.brushGate]), while the host holds a stroke of the brush's pan.
 */
@Composable
internal fun JsCoverGrid(
    widget: JsWidget.CoverGrid,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val columns = widget.columns.coerceIn(1, JsCoverCells.MAX_SIDE)
    val rows = widget.rows.coerceIn(1, JsCoverCells.MAX_SIDE)
    val treeCells = remember(widget.cleared) { BitSet().apply { widget.cleared.filter { it >= 0 }.forEach(::set) } }
    val since = widget.cells
    val brush = widget.brush
    val mirrored = LocalLayoutDirection.current == LayoutDirection.Rtl
    val holesLayer = rememberGraphicsLayer()
    val placed = remember { arrayOfNulls<LayoutCoordinates>(1) }
    Box(
        modifier = modifier
            // Offscreen, so clearing reaches through the cover only and never into what is drawn behind the node.
            .graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen)
            .onPlaced { placed[0] = it }
            .drawWithContent {
                drawContent()
                val worker = since?.drawn?.cells
                val strokes = brush?.trail?.drawn?.strokes.orEmpty()
                val brushed = brush != null && strokes.isNotEmpty() && worker != null && !worker.isEmpty
                val cells = when {
                    brushed || worker == null -> treeCells
                    else -> (treeCells.clone() as BitSet).apply { or(worker) }
                }
                if (brushed && brush != null && worker != null) {
                    val origin = brush.trail?.frame?.let { pan -> placed[0]?.let { pan.originOf(it) } } ?: Offset.Zero
                    val gate = Path()
                    CoverGridRules.brushGate(worker, columns, rows, size, mirrored).forEach(gate::addOval)
                    val line = Path()
                    CoverGridRules.brushPoints(strokes, origin.x, origin.y).forEach { points ->
                        line.moveTo(points[0], points[1])
                        for (i in 2 until points.size step 2) line.lineTo(points[i], points[i + 1])
                    }
                    clipPath(gate) {
                        drawPath(
                            line,
                            Color.Black,
                            style = Stroke(width = brush.width * density, cap = StrokeCap.Round, join = StrokeJoin.Round),
                            blendMode = BlendMode.Clear,
                        )
                    }
                }
                if (cells.isEmpty) return@drawWithContent
                val holes = Path()
                CoverGridRules.holes(cells, columns, rows, size, mirrored).forEach(holes::addOval)
                val softening = CoverGridRules.softening(columns, rows, size)
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && softening > 0f) {
                    holesLayer.renderEffect = smoothEdge(softening)
                    holesLayer.blendMode = BlendMode.DstOut
                    holesLayer.record { drawPath(holes, Color.Black) }
                    drawLayer(holesLayer)
                } else {
                    drawPath(holes, Color.Black, blendMode = BlendMode.Clear)
                }
            },
    ) {
        content()
    }
}

/** Softens the holes by [softening] pixels, then sharpens their edge again ([CoverGridRules.sharpenEdge]). */
@RequiresApi(Build.VERSION_CODES.S)
private fun smoothEdge(softening: Float): ComposeRenderEffect {
    val radius = CoverGridRules.blurRadius(softening)
    return RenderEffect.createChainEffect(
        RenderEffect.createColorFilterEffect(ColorMatrixColorFilter(CoverGridRules.sharpenEdge())),
        RenderEffect.createBlurEffect(radius, radius, Shader.TileMode.DECAL),
    ).asComposeRenderEffect()
}

/** Where [cover]'s top-left corner lies in this pan's frame; zero while either is not laid out. */
private fun LayoutCoordinates.originOf(cover: LayoutCoordinates): Offset =
    if (isAttached && cover.isAttached) localPositionOf(cover, Offset.Zero) else Offset.Zero

/** A cover grid's geometry, kept apart from drawing so it can be checked on its own. */
internal object CoverGridRules {
    /**
     * One circle per cleared cell: centred on the cell, its radius half the cell's diagonal, so it reaches the cell's
     * corners and neighbouring holes merge into one soft-edged scratch with no cover left between them. Column 0 sits
     * at the start edge; [mirrored] puts it on the right for right-to-left layouts. Cells past the grid draw nothing.
     */
    fun holes(cells: BitSet, columns: Int, rows: Int, size: Size, mirrored: Boolean = false): List<Rect> {
        if (columns <= 0 || rows <= 0 || size.width <= 0f || size.height <= 0f) return emptyList()
        val width = size.width / columns
        val height = size.height / rows
        val radius = 0.5f * sqrt(width * width + height * height)
        val out = ArrayList<Rect>(cells.cardinality())
        var cell = cells.nextSetBit(0)
        while (cell >= 0 && cell < columns * rows) {
            val centreX = (cell % columns + 0.5f) * width
            val x = if (mirrored) size.width - centreX else centreX
            val y = (cell / columns + 0.5f) * height
            out += Rect(x - radius, y - radius, x + radius, y + radius)
            cell = cells.nextSetBit(cell + 1)
        }
        return out
    }

    /**
     * Where a brush may cut: a circle one cell's diagonal wide around each of [cells], so a stroke whose worker cleared
     * every cell within half the brush of the finger's path is cut whole, and nothing is cut more than a diagonal from
     * a cleared cell.
     */
    fun brushGate(cells: BitSet, columns: Int, rows: Int, size: Size, mirrored: Boolean = false): List<Rect> {
        if (columns <= 0 || rows <= 0 || size.width <= 0f || size.height <= 0f) return emptyList()
        val width = size.width / columns
        val height = size.height / rows
        val radius = sqrt(width * width + height * height)
        val out = ArrayList<Rect>(cells.cardinality())
        var cell = cells.nextSetBit(0)
        while (cell >= 0 && cell < columns * rows) {
            val centreX = (cell % columns + 0.5f) * width
            val x = if (mirrored) size.width - centreX else centreX
            val y = (cell / columns + 0.5f) * height
            out += Rect(x - radius, y - radius, x + radius, y + radius)
            cell = cells.nextSetBit(cell + 1)
        }
        return out
    }

    /**
     * The finger's strokes (x, y pairs in the pan's frame) moved into the cover's frame, whose top-left corner lies at
     * [originX], [originY] in the pan's. A stroke of one point becomes a line too short to see, which round caps draw
     * as a dot the brush's width, as the existing card does.
     */
    fun brushPoints(strokes: List<FloatArray>, originX: Float = 0f, originY: Float = 0f): List<FloatArray> =
        strokes.filter { it.size >= 2 }.map { stroke ->
            val points = if (stroke.size == 2) floatArrayOf(stroke[0] - DOT, stroke[1], stroke[0], stroke[1]) else stroke
            FloatArray(points.size) { i -> points[i] - if (i % 2 == 0) originX else originY }
        }

    private const val DOT = 0.1f

    /**
     * Whether a brush [width] wide with round caps and joins cuts the point [x], [y] along [strokes]: it lies within
     * half the width of one of their segments. This is what the drawn line covers.
     */
    fun brushCuts(x: Float, y: Float, strokes: List<FloatArray>, width: Float): Boolean {
        val reach = width / 2
        return brushPoints(strokes).any { points ->
            (0 until points.size / 2 - 1).any { i ->
                distance2ToSegment(x, y, points[2 * i], points[2 * i + 1], points[2 * i + 2], points[2 * i + 3]) <= reach * reach
            }
        }
    }

    private fun distance2ToSegment(x: Float, y: Float, ax: Float, ay: Float, bx: Float, by: Float): Float {
        val dx = bx - ax
        val dy = by - ay
        val length2 = dx * dx + dy * dy
        val t = if (length2 == 0f) 0f else (((x - ax) * dx + (y - ay) * dy) / length2).coerceIn(0f, 1f)
        val px = ax + t * dx - x
        val py = ay + t * dy - y
        return px * px + py * py
    }

    /** How steeply the softened edge is sharpened: opacity is stretched this many times about one half. */
    const val EDGE_GAIN = 16f

    /**
     * How far the holes' edge is softened before it is sharpened again: half the cell's shorter side. That melts the
     * steps between neighbouring cells and the scallops between round holes into one round edge, and the edge stays
     * where the softened holes are half clear, so a scratch keeps its width. Zero when there is no grid to smooth.
     */
    fun softening(columns: Int, rows: Int, size: Size): Float {
        if (columns <= 0 || rows <= 0 || size.width <= 0f || size.height <= 0f) return 0f
        return 0.5f * minOf(size.width / columns, size.height / rows)
    }

    /**
     * The blur radius Android needs for a softening of [softening] pixels. Android turns a blur radius into the
     * Gaussian's spread as 0.57735 x radius + 0.5, so the radius is taken back from the spread wanted; iOS takes the
     * spread itself.
     */
    fun blurRadius(softening: Float): Float = maxOf(0f, (softening - BLUR_SPREAD_OFFSET) / BLUR_SPREAD_PER_RADIUS)

    private const val BLUR_SPREAD_PER_RADIUS = 0.57735f
    private const val BLUR_SPREAD_OFFSET = 0.5f

    /**
     * A colour matrix (4 x 5, offsets in 0..255) that leaves colour alone and stretches opacity [EDGE_GAIN] times
     * about one half, so the softened edge ends about a point wide, smooth but not blurred.
     */
    fun sharpenEdge(): FloatArray = floatArrayOf(
        1f, 0f, 0f, 0f, 0f,
        0f, 1f, 0f, 0f, 0f,
        0f, 0f, 1f, 0f, 0f,
        0f, 0f, 0f, EDGE_GAIN, 255f * 0.5f * (1f - EDGE_GAIN),
    )
}
