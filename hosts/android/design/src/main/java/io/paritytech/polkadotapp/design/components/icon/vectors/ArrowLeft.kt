package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ArrowLeft: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ArrowLeft",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.White)) {
            moveTo(11.293f, 4.29289f)
            curveTo(11.6835f, 3.90237f, 12.3165f, 3.90237f, 12.707f, 4.29289f)
            curveTo(13.0975f, 4.68342f, 13.0975f, 5.31643f, 12.707f, 5.70696f)
            lineTo(7.41405f, 10.9999f)
            horizontalLineTo(19f)
            curveTo(19.5523f, 10.9999f, 20f, 11.4476f, 20f, 11.9999f)
            curveTo(20f, 12.5522f, 19.5523f, 12.9999f, 19f, 12.9999f)
            horizontalLineTo(7.41405f)
            lineTo(12.707f, 18.2929f)
            curveTo(13.0975f, 18.6834f, 13.0975f, 19.3164f, 12.707f, 19.707f)
            curveTo(12.3165f, 20.0975f, 11.6835f, 20.0975f, 11.293f, 19.707f)
            lineTo(4.29295f, 12.707f)
            curveTo(3.90243f, 12.3164f, 3.90243f, 11.6834f, 4.29295f, 11.2929f)
            lineTo(11.293f, 4.29289f)
            close()
        }
    }.build()
}
