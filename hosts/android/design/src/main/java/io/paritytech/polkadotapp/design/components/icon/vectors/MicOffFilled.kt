package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.MicOffFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "MicOffFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(19f, 11.5f)
            horizontalLineTo(17.3f)
            curveTo(17.3f, 12.24f, 17.14f, 12.93f, 16.87f, 13.55f)
            lineTo(18.1f, 14.78f)
            curveTo(18.66f, 13.8f, 19f, 12.69f, 19f, 11.5f)
            close()
            moveTo(14.98f, 11.67f)
            curveTo(14.98f, 11.61f, 15f, 11.56f, 15f, 11.5f)
            verticalLineTo(5.5f)
            curveTo(15f, 3.84f, 13.66f, 2.5f, 12f, 2.5f)
            curveTo(10.34f, 2.5f, 9f, 3.84f, 9f, 5.5f)
            verticalLineTo(5.68f)
            lineTo(14.98f, 11.67f)
            close()
            moveTo(4.27f, 3.5f)
            lineTo(3f, 4.77f)
            lineTo(9.01f, 10.78f)
            verticalLineTo(11.5f)
            curveTo(9.01f, 13.16f, 10.34f, 14.5f, 12f, 14.5f)
            curveTo(12.22f, 14.5f, 12.44f, 14.47f, 12.65f, 14.42f)
            lineTo(14.31f, 16.08f)
            curveTo(13.6f, 16.41f, 12.81f, 16.6f, 12f, 16.6f)
            curveTo(9.24f, 16.6f, 6.7f, 14.5f, 6.7f, 11.5f)
            horizontalLineTo(5f)
            curveTo(5f, 14.91f, 7.72f, 17.73f, 11f, 18.22f)
            verticalLineTo(21.5f)
            horizontalLineTo(13f)
            verticalLineTo(18.22f)
            curveTo(13.91f, 18.09f, 14.77f, 17.77f, 15.54f, 17.32f)
            lineTo(19.73f, 21.5f)
            lineTo(21f, 20.23f)
            lineTo(4.27f, 3.5f)
            close()
        }
    }.build()
}
