package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathData
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Cloud: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Cloud",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        group(
            clipPathData = PathData {
                moveTo(1.5f, 1.5f)
                horizontalLineToRelative(21f)
                verticalLineToRelative(21f)
                horizontalLineToRelative(-21f)
                close()
            }
        ) {
            path(fill = SolidColor(Color(0xFFFFFFFF))) {
                moveTo(12f, 6.75f)
                curveTo(14.284f, 6.75f, 16.279f, 8.377f, 16.725f, 10.626f)
                lineTo(16.987f, 11.939f)
                lineTo(18.317f, 12.035f)
                curveTo(19.683f, 12.131f, 20.75f, 13.269f, 20.75f, 14.625f)
                curveTo(20.75f, 16.069f, 19.569f, 17.25f, 18.125f, 17.25f)
                horizontalLineTo(6.75f)
                curveTo(4.816f, 17.25f, 3.25f, 15.684f, 3.25f, 13.75f)
                curveTo(3.25f, 11.956f, 4.589f, 10.46f, 6.365f, 10.276f)
                lineTo(7.301f, 10.18f)
                lineTo(7.739f, 9.349f)
                curveTo(8.561f, 7.747f, 10.198f, 6.75f, 12f, 6.75f)
                close()
                moveTo(12f, 5f)
                curveTo(9.471f, 5f, 7.266f, 6.435f, 6.173f, 8.535f)
                curveTo(3.547f, 8.815f, 1.5f, 11.046f, 1.5f, 13.75f)
                curveTo(1.5f, 16.646f, 3.854f, 19f, 6.75f, 19f)
                horizontalLineTo(18.125f)
                curveTo(20.54f, 19f, 22.5f, 17.04f, 22.5f, 14.625f)
                curveTo(22.5f, 12.315f, 20.706f, 10.443f, 18.44f, 10.285f)
                curveTo(17.836f, 7.266f, 15.185f, 5f, 12f, 5f)
                close()
            }
        }
    }.build()
}
