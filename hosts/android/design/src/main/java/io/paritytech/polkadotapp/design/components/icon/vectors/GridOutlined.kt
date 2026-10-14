package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.GridOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "GridOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF080808))) {
            moveTo(4f, 15f)
            verticalLineTo(20f)
            horizontalLineTo(9f)
            verticalLineTo(15f)
            horizontalLineTo(4f)
            close()
            moveTo(15f, 15f)
            verticalLineTo(20f)
            horizontalLineTo(20f)
            verticalLineTo(15f)
            horizontalLineTo(15f)
            close()
            moveTo(4f, 4f)
            verticalLineTo(9f)
            horizontalLineTo(9f)
            verticalLineTo(4f)
            horizontalLineTo(4f)
            close()
            moveTo(15f, 4f)
            verticalLineTo(9f)
            horizontalLineTo(20f)
            verticalLineTo(4f)
            horizontalLineTo(15f)
            close()
            moveTo(11f, 20f)
            curveTo(11f, 21.105f, 10.105f, 22f, 9f, 22f)
            horizontalLineTo(4f)
            curveTo(2.895f, 22f, 2f, 21.105f, 2f, 20f)
            verticalLineTo(15f)
            curveTo(2f, 13.895f, 2.895f, 13f, 4f, 13f)
            horizontalLineTo(9f)
            curveTo(10.105f, 13f, 11f, 13.895f, 11f, 15f)
            verticalLineTo(20f)
            close()
            moveTo(22f, 20f)
            curveTo(22f, 21.105f, 21.105f, 22f, 20f, 22f)
            horizontalLineTo(15f)
            curveTo(13.895f, 22f, 13f, 21.105f, 13f, 20f)
            verticalLineTo(15f)
            curveTo(13f, 13.895f, 13.895f, 13f, 15f, 13f)
            horizontalLineTo(20f)
            curveTo(21.105f, 13f, 22f, 13.895f, 22f, 15f)
            verticalLineTo(20f)
            close()
            moveTo(11f, 9f)
            curveTo(11f, 10.105f, 10.105f, 11f, 9f, 11f)
            horizontalLineTo(4f)
            curveTo(2.895f, 11f, 2f, 10.105f, 2f, 9f)
            verticalLineTo(4f)
            curveTo(2f, 2.895f, 2.895f, 2f, 4f, 2f)
            horizontalLineTo(9f)
            curveTo(10.105f, 2f, 11f, 2.895f, 11f, 4f)
            verticalLineTo(9f)
            close()
            moveTo(22f, 9f)
            curveTo(22f, 10.105f, 21.105f, 11f, 20f, 11f)
            horizontalLineTo(15f)
            curveTo(13.895f, 11f, 13f, 10.105f, 13f, 9f)
            verticalLineTo(4f)
            curveTo(13f, 2.895f, 13.895f, 2f, 15f, 2f)
            horizontalLineTo(20f)
            curveTo(21.105f, 2f, 22f, 2.895f, 22f, 4f)
            verticalLineTo(9f)
            close()
        }
    }.build()
}
