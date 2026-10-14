package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathData
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val CallStatusPositive: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CallStatusPositive",
        defaultWidth = 16.dp,
        defaultHeight = 16.dp,
        viewportWidth = 16f,
        viewportHeight = 16f
    ).apply {
        group(
            clipPathData = PathData {
                moveTo(0f, 0f)
                horizontalLineToRelative(16f)
                verticalLineToRelative(16f)
                horizontalLineToRelative(-16f)
                close()
            }
        ) {
            path(fill = SolidColor(Color(0xFF828282))) {
                moveTo(12f, 11.333f)
                curveTo(12f, 11.701f, 11.701f, 12f, 11.333f, 12f)
                curveTo(10.965f, 12f, 10.667f, 11.701f, 10.667f, 11.333f)
                verticalLineTo(6.276f)
                lineTo(5.138f, 11.805f)
                curveTo(4.878f, 12.065f, 4.456f, 12.065f, 4.195f, 11.805f)
                curveTo(3.935f, 11.544f, 3.935f, 11.122f, 4.195f, 10.862f)
                lineTo(9.724f, 5.333f)
                horizontalLineTo(4.667f)
                curveTo(4.298f, 5.333f, 4f, 5.035f, 4f, 4.667f)
                curveTo(4f, 4.298f, 4.298f, 4f, 4.667f, 4f)
                horizontalLineTo(11.333f)
                curveTo(11.701f, 4f, 12f, 4.298f, 12f, 4.667f)
                verticalLineTo(11.333f)
                close()
            }
        }
    }.build()
}
