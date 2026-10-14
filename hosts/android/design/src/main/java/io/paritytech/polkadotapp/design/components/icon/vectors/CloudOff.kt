package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.CloudOff: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CloudOff",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(24f, 14.07f)
            curveTo(24f, 11.43f, 21.95f, 9.29f, 19.35f, 9.11f)
            curveTo(18.67f, 5.66f, 15.64f, 3.07f, 12f, 3.07f)
            curveTo(10.67f, 3.07f, 9.43f, 3.43f, 8.35f, 4.04f)
            lineTo(9.84f, 5.53f)
            curveTo(10.51f, 5.24f, 11.23f, 5.07f, 12f, 5.07f)
            curveTo(15.04f, 5.07f, 17.5f, 7.53f, 17.5f, 10.57f)
            verticalLineTo(11.07f)
            horizontalLineTo(19f)
            curveTo(20.66f, 11.07f, 22f, 12.41f, 22f, 14.07f)
            curveTo(22f, 15.06f, 21.52f, 15.92f, 20.79f, 16.47f)
            lineTo(22.2f, 17.88f)
            curveTo(23.29f, 16.96f, 24f, 15.61f, 24f, 14.07f)
            close()
            moveTo(4.41f, 2.93f)
            lineTo(3f, 4.34f)
            lineTo(5.77f, 7.11f)
            horizontalLineTo(5.35f)
            curveTo(2.34f, 7.43f, 0f, 9.98f, 0f, 13.07f)
            curveTo(0f, 16.38f, 2.69f, 19.07f, 6f, 19.07f)
            horizontalLineTo(17.73f)
            lineTo(19.73f, 21.07f)
            lineTo(21.14f, 19.66f)
            lineTo(4.41f, 2.93f)
            close()
            moveTo(6f, 17.07f)
            curveTo(3.79f, 17.07f, 2f, 15.28f, 2f, 13.07f)
            curveTo(2f, 10.86f, 3.79f, 9.07f, 6f, 9.07f)
            horizontalLineTo(7.73f)
            lineTo(15.73f, 17.07f)
            horizontalLineTo(6f)
            close()
        }
    }.build()
}
