package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection

object DockShape : Shape {
    override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density): Outline =
        Outline.Generic(dockPath(size, density.density))
}

class DockletCardShape(private val pillShare: () -> Float) : Shape {
    override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density): Outline =
        Outline.Generic(cardPath(size, density.density, pillShare()))
}

internal fun dockPath(size: Size, density: Float): Path {
    val width = size.width
    val height = size.height
    val radius = height / 2

    return Path().apply {
        moveTo(radius, 0f)
        addCutout(width, density)
        lineTo(width - radius, 0f)
        arcTo(Rect(width - height, 0f, width, height), -90f, 180f, false)
        lineTo(radius, height)
        arcTo(Rect(0f, 0f, height, height), 90f, 180f, false)
        close()
    }
}

internal fun cardPath(size: Size, density: Float, pillShare: Float): Path {
    val bodyHeight = size.height / density - DockletGeometry.LobeDepth.value
    val outline = DockletGeometry.cardOutlineCurves(size.width / density, bodyHeight, pillShare)
    return Path().apply {
        moveTo(outline.start.x * density, outline.start.y * density)
        addSegments(outline.segments, density)
        close()
    }
}

private fun Path.addCutout(width: Float, density: Float) {
    val cutout = DockletGeometry.cutout(width / density / 2)
    lineTo(cutout.start.x * density, cutout.start.y * density)
    addSegments(cutout.segments, density)
}

private fun Path.addSegments(segments: List<CubicSegment>, density: Float) {
    segments.forEach { segment ->
        cubicTo(
            segment.control1.x * density,
            segment.control1.y * density,
            segment.control2.x * density,
            segment.control2.y * density,
            segment.end.x * density,
            segment.end.y * density,
        )
    }
}
