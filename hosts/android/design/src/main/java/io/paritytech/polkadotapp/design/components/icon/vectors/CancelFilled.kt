package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.CancelFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CancelFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 2f)
            curveTo(6.47f, 2f, 2f, 6.47f, 2f, 12f)
            curveTo(2f, 17.53f, 6.47f, 22f, 12f, 22f)
            curveTo(17.53f, 22f, 22f, 17.53f, 22f, 12f)
            curveTo(22f, 6.47f, 17.53f, 2f, 12f, 2f)
            close()
            moveTo(17f, 15.59f)
            lineTo(15.59f, 17f)
            lineTo(12f, 13.41f)
            lineTo(8.41f, 17f)
            lineTo(7f, 15.59f)
            lineTo(10.59f, 12f)
            lineTo(7f, 8.41f)
            lineTo(8.41f, 7f)
            lineTo(12f, 10.59f)
            lineTo(15.59f, 7f)
            lineTo(17f, 8.41f)
            lineTo(13.41f, 12f)
            lineTo(17f, 15.59f)
            close()
        }
    }.build()
}
