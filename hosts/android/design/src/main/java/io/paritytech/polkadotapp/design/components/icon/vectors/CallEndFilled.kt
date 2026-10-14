package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.CallEndFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CallEndFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 9.57f)
            curveTo(10.4f, 9.57f, 8.85f, 9.82f, 7.4f, 10.29f)
            verticalLineTo(13.39f)
            curveTo(7.4f, 13.78f, 7.17f, 14.13f, 6.84f, 14.29f)
            curveTo(5.86f, 14.78f, 4.97f, 15.41f, 4.18f, 16.14f)
            curveTo(4f, 16.32f, 3.75f, 16.42f, 3.48f, 16.42f)
            curveTo(3.2f, 16.42f, 2.95f, 16.31f, 2.77f, 16.13f)
            lineTo(0.29f, 13.65f)
            curveTo(0.11f, 13.48f, 0f, 13.23f, 0f, 12.95f)
            curveTo(0f, 12.67f, 0.11f, 12.42f, 0.29f, 12.24f)
            curveTo(3.34f, 9.35f, 7.46f, 7.57f, 12f, 7.57f)
            curveTo(16.54f, 7.57f, 20.66f, 9.35f, 23.71f, 12.24f)
            curveTo(23.89f, 12.42f, 24f, 12.67f, 24f, 12.95f)
            curveTo(24f, 13.23f, 23.89f, 13.48f, 23.71f, 13.66f)
            lineTo(21.23f, 16.14f)
            curveTo(21.05f, 16.32f, 20.8f, 16.43f, 20.52f, 16.43f)
            curveTo(20.25f, 16.43f, 20f, 16.32f, 19.82f, 16.15f)
            curveTo(19.03f, 15.41f, 18.13f, 14.79f, 17.15f, 14.3f)
            curveTo(16.82f, 14.14f, 16.59f, 13.8f, 16.59f, 13.4f)
            verticalLineTo(10.3f)
            curveTo(15.15f, 9.82f, 13.6f, 9.57f, 12f, 9.57f)
            close()
        }
    }.build()
}
