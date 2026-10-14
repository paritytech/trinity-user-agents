package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Close: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Close",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(
            fill = SolidColor(Color(0xFFFFFFFF)),
            pathFillType = PathFillType.EvenOdd
        ) {
            moveTo(4.293f, 5.707f)
            curveTo(3.902f, 5.317f, 3.902f, 4.683f, 4.293f, 4.293f)
            curveTo(4.683f, 3.902f, 5.317f, 3.902f, 5.707f, 4.293f)
            lineTo(12f, 10.586f)
            lineTo(18.293f, 4.293f)
            curveTo(18.683f, 3.902f, 19.317f, 3.902f, 19.707f, 4.293f)
            curveTo(20.098f, 4.683f, 20.098f, 5.317f, 19.707f, 5.707f)
            lineTo(13.414f, 12f)
            lineTo(19.707f, 18.293f)
            curveTo(20.098f, 18.683f, 20.098f, 19.317f, 19.707f, 19.707f)
            curveTo(19.317f, 20.098f, 18.683f, 20.098f, 18.293f, 19.707f)
            lineTo(12f, 13.414f)
            lineTo(5.707f, 19.707f)
            curveTo(5.317f, 20.098f, 4.683f, 20.098f, 4.293f, 19.707f)
            curveTo(3.902f, 19.317f, 3.902f, 18.683f, 4.293f, 18.293f)
            lineTo(10.586f, 12f)
            lineTo(4.293f, 5.707f)
            close()
        }
    }.build()
}
