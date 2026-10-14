package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ChargingFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ChargingFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(14.5f, 11f)
            lineTo(11.5f, 17f)
            verticalLineTo(13f)
            horizontalLineTo(9.5f)
            lineTo(12.5f, 7f)
            verticalLineTo(11f)
            horizontalLineTo(14.5f)
            close()
            moveTo(7f, 1f)
            horizontalLineTo(17f)
            curveTo(18.1f, 1f, 19f, 1.9f, 19f, 3f)
            verticalLineTo(21f)
            curveTo(19f, 22.1f, 18.1f, 23f, 17f, 23f)
            horizontalLineTo(7f)
            curveTo(5.9f, 23f, 5f, 22.1f, 5f, 21f)
            verticalLineTo(3f)
            curveTo(5f, 1.9f, 5.9f, 1f, 7f, 1f)
            close()
            moveTo(7f, 6f)
            verticalLineTo(18f)
            horizontalLineTo(17f)
            verticalLineTo(6f)
            horizontalLineTo(7f)
            close()
        }
    }.build()
}
