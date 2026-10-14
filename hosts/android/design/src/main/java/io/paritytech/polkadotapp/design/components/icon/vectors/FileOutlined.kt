package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.FileOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "FileOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF080808))) {
            moveTo(3f, 20f)
            verticalLineTo(4f)
            curveTo(3f, 3.204f, 3.316f, 2.442f, 3.879f, 1.879f)
            curveTo(4.442f, 1.316f, 5.204f, 1f, 6f, 1f)
            horizontalLineTo(14.5f)
            curveTo(14.765f, 1f, 15.019f, 1.105f, 15.207f, 1.293f)
            lineTo(20.707f, 6.793f)
            curveTo(20.895f, 6.981f, 21f, 7.235f, 21f, 7.5f)
            verticalLineTo(20f)
            curveTo(21f, 20.796f, 20.684f, 21.559f, 20.121f, 22.121f)
            curveTo(19.559f, 22.684f, 18.796f, 23f, 18f, 23f)
            horizontalLineTo(6f)
            curveTo(5.204f, 23f, 4.442f, 22.684f, 3.879f, 22.121f)
            curveTo(3.316f, 21.559f, 3f, 20.796f, 3f, 20f)
            close()
            moveTo(15f, 7f)
            horizontalLineTo(18.086f)
            lineTo(15f, 3.914f)
            verticalLineTo(7f)
            close()
            moveTo(5f, 20f)
            curveTo(5f, 20.265f, 5.105f, 20.52f, 5.293f, 20.707f)
            curveTo(5.481f, 20.895f, 5.735f, 21f, 6f, 21f)
            horizontalLineTo(18f)
            curveTo(18.265f, 21f, 18.52f, 20.895f, 18.707f, 20.707f)
            curveTo(18.895f, 20.52f, 19f, 20.265f, 19f, 20f)
            verticalLineTo(9f)
            horizontalLineTo(14f)
            curveTo(13.448f, 9f, 13f, 8.552f, 13f, 8f)
            verticalLineTo(3f)
            horizontalLineTo(6f)
            curveTo(5.735f, 3f, 5.481f, 3.105f, 5.293f, 3.293f)
            curveTo(5.105f, 3.48f, 5f, 3.735f, 5f, 4f)
            verticalLineTo(20f)
            close()
        }
    }.build()
}
