package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Refreshing: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "StyleOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(19f, 8f)
            lineTo(15f, 12f)
            horizontalLineTo(18f)
            curveTo(18f, 15.31f, 15.31f, 18f, 12f, 18f)
            curveTo(10.99f, 18f, 10.03f, 17.75f, 9.2f, 17.3f)
            lineTo(7.74f, 18.76f)
            curveTo(8.97f, 19.54f, 10.43f, 20f, 12f, 20f)
            curveTo(16.42f, 20f, 20f, 16.42f, 20f, 12f)
            horizontalLineTo(23f)
            lineTo(19f, 8f)
            close()
            moveTo(6f, 12f)
            curveTo(6f, 8.69f, 8.69f, 6f, 12f, 6f)
            curveTo(13.01f, 6f, 13.97f, 6.25f, 14.8f, 6.7f)
            lineTo(16.26f, 5.24f)
            curveTo(15.03f, 4.46f, 13.57f, 4f, 12f, 4f)
            curveTo(7.58f, 4f, 4f, 7.58f, 4f, 12f)
            horizontalLineTo(1f)
            lineTo(5f, 16f)
            lineTo(9f, 12f)
            horizontalLineTo(6f)
            close()
        }
    }.build()
}
