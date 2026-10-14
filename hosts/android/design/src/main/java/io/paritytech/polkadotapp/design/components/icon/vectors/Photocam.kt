package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Photocam: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Photocam",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(14.12f, 5f)
            lineTo(15.95f, 7f)
            horizontalLineTo(20f)
            verticalLineTo(19f)
            horizontalLineTo(4f)
            verticalLineTo(7f)
            horizontalLineTo(8.05f)
            lineTo(9.88f, 5f)
            horizontalLineTo(14.12f)
            close()
            moveTo(15f, 3f)
            horizontalLineTo(9f)
            lineTo(7.17f, 5f)
            horizontalLineTo(4f)
            curveTo(2.9f, 5f, 2f, 5.9f, 2f, 7f)
            verticalLineTo(19f)
            curveTo(2f, 20.1f, 2.9f, 21f, 4f, 21f)
            horizontalLineTo(20f)
            curveTo(21.1f, 21f, 22f, 20.1f, 22f, 19f)
            verticalLineTo(7f)
            curveTo(22f, 5.9f, 21.1f, 5f, 20f, 5f)
            horizontalLineTo(16.83f)
            lineTo(15f, 3f)
            close()
            moveTo(12f, 10f)
            curveTo(13.65f, 10f, 15f, 11.35f, 15f, 13f)
            curveTo(15f, 14.65f, 13.65f, 16f, 12f, 16f)
            curveTo(10.35f, 16f, 9f, 14.65f, 9f, 13f)
            curveTo(9f, 11.35f, 10.35f, 10f, 12f, 10f)
            close()
            moveTo(12f, 8f)
            curveTo(9.24f, 8f, 7f, 10.24f, 7f, 13f)
            curveTo(7f, 15.76f, 9.24f, 18f, 12f, 18f)
            curveTo(14.76f, 18f, 17f, 15.76f, 17f, 13f)
            curveTo(17f, 10.24f, 14.76f, 8f, 12f, 8f)
            close()
        }
    }.build()
}
