package io.paritytech.polkadotapp.design.components.tooltip.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathData
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val TriangleLeftIcon: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "TooltipTriangleLeft",
        defaultWidth = 22.dp,
        defaultHeight = 28.dp,
        viewportWidth = 22f,
        viewportHeight = 28f
    ).apply {
        group(
            rotate = 90f,
            pivotX = 11f,
            pivotY = 14f,
            clipPathData = PathData {
                moveTo(-2.857f, 3.125f)
                horizontalLineToRelative(27.713f)
                verticalLineToRelative(21.75f)
                horizontalLineToRelative(-27.713f)
                close()
            }
        ) {
            path(fill = SolidColor(Color.White)) {
                moveTo(13.53f, 17.903f)
                curveTo(12.351f, 19.754f, 9.648f, 19.754f, 8.469f, 17.903f)
                lineTo(-2.857f, 0.125f)
                lineTo(24.856f, 0.125f)
                lineTo(13.53f, 17.903f)
                close()
            }
        }
    }.build()
}
