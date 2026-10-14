package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.NotificationActiveOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "NotificationActiveOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 21.75f)
            curveTo(13.1f, 21.75f, 14f, 20.85f, 14f, 19.75f)
            horizontalLineTo(10f)
            curveTo(10f, 20.85f, 10.9f, 21.75f, 12f, 21.75f)
            close()
            moveTo(18f, 15.75f)
            verticalLineTo(10.75f)
            curveTo(18f, 7.68f, 16.37f, 5.11f, 13.5f, 4.43f)
            verticalLineTo(3.75f)
            curveTo(13.5f, 2.92f, 12.83f, 2.25f, 12f, 2.25f)
            curveTo(11.17f, 2.25f, 10.5f, 2.92f, 10.5f, 3.75f)
            verticalLineTo(4.43f)
            curveTo(7.64f, 5.11f, 6f, 7.67f, 6f, 10.75f)
            verticalLineTo(15.75f)
            lineTo(4f, 17.75f)
            verticalLineTo(18.75f)
            horizontalLineTo(20f)
            verticalLineTo(17.75f)
            lineTo(18f, 15.75f)
            close()
            moveTo(16f, 16.75f)
            horizontalLineTo(8f)
            verticalLineTo(10.75f)
            curveTo(8f, 8.27f, 9.51f, 6.25f, 12f, 6.25f)
            curveTo(14.49f, 6.25f, 16f, 8.27f, 16f, 10.75f)
            verticalLineTo(16.75f)
            close()
            moveTo(7.58f, 3.83f)
            lineTo(6.15f, 2.4f)
            curveTo(3.75f, 4.23f, 2.17f, 7.05f, 2.03f, 10.25f)
            horizontalLineTo(4.03f)
            curveTo(4.18f, 7.6f, 5.54f, 5.28f, 7.58f, 3.83f)
            close()
            moveTo(19.97f, 10.25f)
            horizontalLineTo(21.97f)
            curveTo(21.82f, 7.05f, 20.24f, 4.23f, 17.85f, 2.4f)
            lineTo(16.43f, 3.83f)
            curveTo(18.45f, 5.28f, 19.82f, 7.6f, 19.97f, 10.25f)
            close()
        }
    }.build()
}
