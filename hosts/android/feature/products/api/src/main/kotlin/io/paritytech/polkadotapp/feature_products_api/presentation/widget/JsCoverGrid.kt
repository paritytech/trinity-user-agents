package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import io.paritytech.polkadotapp.feature_products_api.model.JsCoverCells
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import java.util.BitSet
import kotlin.math.sqrt

/**
 * Draws the children as a cover, with a round hole cut for every cleared cell: those of the tree, plus those the
 * worker cleared since (until the next tree, which always wins). The worker's cells are read in the draw phase only,
 * so a cleared-cells update redraws the cover and composes nothing.
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
    val since: State<JsCoverCells.Value>? = widget.cells?.value?.collectAsState()
    val mirrored = LocalLayoutDirection.current == LayoutDirection.Rtl
    Box(
        modifier = modifier
            // Offscreen, so clearing reaches through the cover only and never into what is drawn behind the node.
            .graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen)
            .drawWithContent {
                drawContent()
                val cells = since?.value?.cells?.let { worker -> (treeCells.clone() as BitSet).apply { or(worker) } } ?: treeCells
                if (cells.isEmpty) return@drawWithContent
                val holes = Path()
                CoverGridRules.holes(cells, columns, rows, size, mirrored).forEach(holes::addOval)
                drawPath(holes, Color.Black, blendMode = BlendMode.Clear)
            },
    ) {
        content()
    }
}

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
}
