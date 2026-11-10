package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathOperation
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.unit.dp

object DockletMaterials {
    val PillBlurSigma = 24.dp
    val DockBlurSigma = 10.dp
    val BehindCardBlurSigma = 8.dp
    val DockItemsBlurSigma = 3.dp
    const val BehindCardBrightness = 0.7f
    const val DockItemsFadedAlpha = 0.55f
}

private val LineWidth = 1.dp

internal fun DrawScope.drawDockletGlass(path: Path, palette: DockletPalette) {
    drawPath(path, palette.glassFill)
    drawInnerLine(path, palette.glassStroke)
}

fun Modifier.dockletFieldOutline(shape: Shape, palette: DockletPalette): Modifier = drawWithCache {
    val path = Path().apply { addOutline(shape.createOutline(size, layoutDirection, this@drawWithCache)) }
    val line = topLine(path, LineWidth.toPx())
    onDrawWithContent {
        drawContent()
        drawPath(line, palette.fieldLine)
        drawInnerLine(path, palette.fieldOutline)
    }
}

private fun DrawScope.drawInnerLine(path: Path, color: Color) {
    clipPath(path) {
        drawPath(path = path, color = color, style = Stroke(width = LineWidth.toPx() * 2))
    }
}

private fun topLine(path: Path, width: Float): Path {
    val shifted = Path().apply { addPath(path, Offset(0f, width)) }
    return Path().apply { op(path, shifted, PathOperation.Difference) }
}
