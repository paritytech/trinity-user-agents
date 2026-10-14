package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.PollOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "PollOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(18f, 13f)
            horizontalLineTo(17.32f)
            lineTo(15.32f, 15f)
            horizontalLineTo(17.23f)
            lineTo(19f, 17f)
            horizontalLineTo(5f)
            lineTo(6.78f, 15f)
            horizontalLineTo(8.83f)
            lineTo(6.83f, 13f)
            horizontalLineTo(6f)
            lineTo(3f, 16f)
            verticalLineTo(20f)
            curveTo(3f, 21.1f, 3.89f, 22f, 4.99f, 22f)
            horizontalLineTo(19f)
            curveTo(20.1f, 22f, 21f, 21.11f, 21f, 20f)
            verticalLineTo(16f)
            lineTo(18f, 13f)
            close()
            moveTo(19f, 20f)
            horizontalLineTo(5f)
            verticalLineTo(19f)
            horizontalLineTo(19f)
            verticalLineTo(20f)
            close()
            moveTo(11.34f, 15.02f)
            curveTo(11.73f, 15.41f, 12.36f, 15.41f, 12.75f, 15.02f)
            lineTo(19.11f, 8.66f)
            curveTo(19.5f, 8.27f, 19.5f, 7.64f, 19.11f, 7.25f)
            lineTo(14.16f, 2.3f)
            curveTo(13.78f, 1.9f, 13.15f, 1.9f, 12.76f, 2.29f)
            lineTo(6.39f, 8.66f)
            curveTo(6f, 9.05f, 6f, 9.68f, 6.39f, 10.07f)
            lineTo(11.34f, 15.02f)
            close()
            moveTo(13.46f, 4.41f)
            lineTo(17f, 7.95f)
            lineTo(12.05f, 12.9f)
            lineTo(8.51f, 9.36f)
            lineTo(13.46f, 4.41f)
            close()
        }
    }.build()
}
