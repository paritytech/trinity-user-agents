package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.SyncArrowsOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "SyncArrowsOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(7.41f, 13.41f)
            lineTo(6f, 12f)
            lineTo(2f, 16f)
            lineTo(6f, 20f)
            lineTo(7.41f, 18.59f)
            lineTo(5.83f, 17f)
            horizontalLineTo(21f)
            verticalLineTo(15f)
            horizontalLineTo(5.83f)
            lineTo(7.41f, 13.41f)
            close()
        }
        path(fill = SolidColor(Color.Black)) {
            moveTo(16.59f, 10.59f)
            lineTo(18f, 12f)
            lineTo(22f, 8f)
            lineTo(18f, 4f)
            lineTo(16.59f, 5.41f)
            lineTo(18.17f, 7f)
            horizontalLineTo(3f)
            verticalLineTo(9f)
            horizontalLineTo(18.17f)
            lineTo(16.59f, 10.59f)
            close()
        }
    }.build()
}
