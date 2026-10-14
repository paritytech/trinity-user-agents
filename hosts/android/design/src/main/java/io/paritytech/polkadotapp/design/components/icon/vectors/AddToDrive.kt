package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.AddToDrive: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "AddToDrive",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(18.5f, 11f)
            curveTo(18.67f, 11f, 18.83f, 11.01f, 18.99f, 11.02f)
            lineTo(14.5f, 3f)
            horizontalLineTo(8.5f)
            lineTo(14.18f, 12.84f)
            curveTo(15.27f, 11.71f, 16.8f, 11f, 18.5f, 11f)
            close()
        }
        path(fill = SolidColor(Color.Black)) {
            moveTo(7.65f, 4.52f)
            lineTo(1.5f, 15.5f)
            lineTo(4.5f, 21f)
            lineTo(10.83f, 10.03f)
            lineTo(7.65f, 4.52f)
            close()
        }
        path(fill = SolidColor(Color.Black)) {
            moveTo(12.7f, 15.5f)
            horizontalLineTo(9.4f)
            lineTo(6.23f, 21f)
            horizontalLineTo(14.04f)
            curveTo(13.08f, 19.94f, 12.5f, 18.54f, 12.5f, 17f)
            curveTo(12.5f, 16.48f, 12.57f, 15.98f, 12.7f, 15.5f)
            close()
        }
        path(fill = SolidColor(Color.Black)) {
            moveTo(19.5f, 16f)
            verticalLineTo(13f)
            horizontalLineTo(17.5f)
            verticalLineTo(16f)
            horizontalLineTo(14.5f)
            verticalLineTo(18f)
            horizontalLineTo(17.5f)
            verticalLineTo(21f)
            horizontalLineTo(19.5f)
            verticalLineTo(18f)
            horizontalLineTo(22.5f)
            verticalLineTo(16f)
            horizontalLineTo(19.5f)
            close()
        }
    }.build()
}
