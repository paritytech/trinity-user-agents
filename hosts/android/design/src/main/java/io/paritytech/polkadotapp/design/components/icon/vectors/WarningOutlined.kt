package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.WarningOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "WarningOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 6.49f)
            lineTo(19.53f, 19.5f)
            horizontalLineTo(4.47f)
            lineTo(12f, 6.49f)
            close()
            moveTo(12f, 2.5f)
            lineTo(1f, 21.5f)
            horizontalLineTo(23f)
            lineTo(12f, 2.5f)
            close()
        }
        path(fill = SolidColor(Color.Black)) {
            moveTo(13f, 16.5f)
            horizontalLineTo(11f)
            verticalLineTo(18.5f)
            horizontalLineTo(13f)
            verticalLineTo(16.5f)
            close()
        }
        path(fill = SolidColor(Color.Black)) {
            moveTo(13f, 10.5f)
            horizontalLineTo(11f)
            verticalLineTo(15.5f)
            horizontalLineTo(13f)
            verticalLineTo(10.5f)
            close()
        }
    }.build()
}
