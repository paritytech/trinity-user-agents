package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.PhotoSolid: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "PhotoSolid",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        // Glyph authored on a 16-unit grid; scaled into the 24dp design-system canvas.
        group(scaleX = 1.5f, scaleY = 1.5f) {
            path(
                fill = SolidColor(Color(0xFF000000)),
                pathFillType = PathFillType.EvenOdd
            ) {
                moveTo(0.667f, 4.155f)
                curveTo(0.667f, 3.149f, 1.487f, 2.333f, 2.5f, 2.333f)
                horizontalLineTo(13.5f)
                curveTo(14.512f, 2.333f, 15.333f, 3.149f, 15.333f, 4.155f)
                verticalLineTo(11.845f)
                curveTo(15.333f, 12.851f, 14.512f, 13.667f, 13.5f, 13.667f)
                horizontalLineTo(2.5f)
                curveTo(1.487f, 13.667f, 0.667f, 12.851f, 0.667f, 11.845f)
                verticalLineTo(4.155f)
                close()
                moveTo(1.889f, 8.859f)
                verticalLineTo(11.845f)
                curveTo(1.889f, 12.181f, 2.162f, 12.453f, 2.5f, 12.453f)
                horizontalLineTo(13.5f)
                curveTo(13.837f, 12.453f, 14.111f, 12.181f, 14.111f, 11.845f)
                verticalLineTo(9.668f)
                lineTo(12.302f, 7.871f)
                curveTo(12.064f, 7.634f, 11.677f, 7.634f, 11.438f, 7.871f)
                lineTo(9.883f, 9.417f)
                lineTo(10.265f, 9.797f)
                curveTo(10.504f, 10.034f, 10.504f, 10.419f, 10.265f, 10.656f)
                curveTo(10.027f, 10.893f, 9.64f, 10.893f, 9.401f, 10.656f)
                lineTo(5.173f, 6.455f)
                curveTo(4.934f, 6.218f, 4.547f, 6.218f, 4.309f, 6.455f)
                lineTo(1.889f, 8.859f)
                close()
                moveTo(9.63f, 5.572f)
                curveTo(9.63f, 6.019f, 9.265f, 6.381f, 8.815f, 6.381f)
                curveTo(8.365f, 6.381f, 8f, 6.019f, 8f, 5.572f)
                curveTo(8f, 5.124f, 8.365f, 4.762f, 8.815f, 4.762f)
                curveTo(9.265f, 4.762f, 9.63f, 5.124f, 9.63f, 5.572f)
                close()
            }
        }
    }.build()
}
