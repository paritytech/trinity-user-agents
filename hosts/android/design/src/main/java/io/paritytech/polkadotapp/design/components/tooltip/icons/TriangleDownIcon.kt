package io.paritytech.polkadotapp.design.components.tooltip.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathData
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val TriangleDownIcon: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "TooltipTriangle",
        defaultWidth = 28.dp,
        defaultHeight = 22.dp,
        viewportWidth = 28f,
        viewportHeight = 22f
    ).apply {
        group(
            clipPathData = PathData {
                moveTo(0f, 0f)
                horizontalLineToRelative(27.713f)
                verticalLineToRelative(21.75f)
                horizontalLineToRelative(-27.713f)
                close()
            }
        ) {
            path(fill = SolidColor(Color.White)) {
                moveTo(16.387f, 14.778f)
                curveTo(15.208f, 16.629f, 12.505f, 16.629f, 11.326f, 14.778f)
                lineTo(0f, -3f)
                lineTo(27.713f, -3f)
                lineTo(16.387f, 14.778f)
                close()
            }
        }
    }.build()
}
