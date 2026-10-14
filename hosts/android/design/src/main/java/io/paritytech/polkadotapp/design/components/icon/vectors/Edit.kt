package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathData
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Edit: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Edit",
        defaultWidth = 20.dp,
        defaultHeight = 20.dp,
        viewportWidth = 20f,
        viewportHeight = 20f
    ).apply {
        group(
            clipPathData = PathData {
                moveTo(0f, 0f)
                horizontalLineToRelative(20f)
                verticalLineToRelative(20f)
                horizontalLineToRelative(-20f)
                close()
            }
        ) {
            path(fill = SolidColor(Color(0xFF1C1B1F))) {
                moveTo(4.167f, 15.833f)
                horizontalLineTo(5.354f)
                lineTo(13.5f, 7.688f)
                lineTo(12.313f, 6.5f)
                lineTo(4.167f, 14.646f)
                verticalLineTo(15.833f)
                close()
                moveTo(2.5f, 17.5f)
                verticalLineTo(13.958f)
                lineTo(13.5f, 2.979f)
                curveTo(13.667f, 2.826f, 13.851f, 2.708f, 14.052f, 2.625f)
                curveTo(14.253f, 2.542f, 14.465f, 2.5f, 14.688f, 2.5f)
                curveTo(14.91f, 2.5f, 15.125f, 2.542f, 15.333f, 2.625f)
                curveTo(15.542f, 2.708f, 15.722f, 2.833f, 15.875f, 3f)
                lineTo(17.021f, 4.167f)
                curveTo(17.188f, 4.319f, 17.309f, 4.5f, 17.385f, 4.708f)
                curveTo(17.462f, 4.917f, 17.5f, 5.125f, 17.5f, 5.333f)
                curveTo(17.5f, 5.556f, 17.462f, 5.767f, 17.385f, 5.969f)
                curveTo(17.309f, 6.17f, 17.188f, 6.354f, 17.021f, 6.521f)
                lineTo(6.042f, 17.5f)
                horizontalLineTo(2.5f)
                close()
                moveTo(12.896f, 7.104f)
                lineTo(12.313f, 6.5f)
                lineTo(13.5f, 7.688f)
                lineTo(12.896f, 7.104f)
                close()
            }
        }
    }.build()
}
