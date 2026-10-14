package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ContentCopy: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ContentCopy",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(16.5f, 1f)
            horizontalLineTo(4.5f)
            curveTo(3.4f, 1f, 2.5f, 1.9f, 2.5f, 3f)
            verticalLineTo(17f)
            horizontalLineTo(4.5f)
            verticalLineTo(3f)
            horizontalLineTo(16.5f)
            verticalLineTo(1f)
            close()
            moveTo(19.5f, 5f)
            horizontalLineTo(8.5f)
            curveTo(7.4f, 5f, 6.5f, 5.9f, 6.5f, 7f)
            verticalLineTo(21f)
            curveTo(6.5f, 22.1f, 7.4f, 23f, 8.5f, 23f)
            horizontalLineTo(19.5f)
            curveTo(20.6f, 23f, 21.5f, 22.1f, 21.5f, 21f)
            verticalLineTo(7f)
            curveTo(21.5f, 5.9f, 20.6f, 5f, 19.5f, 5f)
            close()
            moveTo(19.5f, 21f)
            horizontalLineTo(8.5f)
            verticalLineTo(7f)
            horizontalLineTo(19.5f)
            verticalLineTo(21f)
            close()
        }
    }.build()
}
