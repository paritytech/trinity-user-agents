package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.CloudOn: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CloudOn",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 6f)
            curveTo(14.62f, 6f, 16.88f, 7.86f, 17.39f, 10.43f)
            lineTo(17.69f, 11.93f)
            lineTo(19.22f, 12.04f)
            curveTo(20.78f, 12.14f, 22f, 13.45f, 22f, 15f)
            curveTo(22f, 16.65f, 20.65f, 18f, 19f, 18f)
            horizontalLineTo(6f)
            curveTo(3.79f, 18f, 2f, 16.21f, 2f, 14f)
            curveTo(2f, 11.95f, 3.53f, 10.24f, 5.56f, 10.03f)
            lineTo(6.63f, 9.92f)
            lineTo(7.13f, 8.97f)
            curveTo(8.08f, 7.14f, 9.94f, 6f, 12f, 6f)
            close()
            moveTo(12f, 4f)
            curveTo(9.11f, 4f, 6.6f, 5.64f, 5.35f, 8.04f)
            curveTo(2.34f, 8.36f, 0f, 10.91f, 0f, 14f)
            curveTo(0f, 17.31f, 2.69f, 20f, 6f, 20f)
            horizontalLineTo(19f)
            curveTo(21.76f, 20f, 24f, 17.76f, 24f, 15f)
            curveTo(24f, 12.36f, 21.95f, 10.22f, 19.35f, 10.04f)
            curveTo(18.67f, 6.59f, 15.64f, 4f, 12f, 4f)
            close()
        }
    }.build()
}
