package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.CallFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CallFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.White)) {
            moveTo(20.01f, 15.38f)
            curveTo(18.78f, 15.38f, 17.59f, 15.18f, 16.48f, 14.82f)
            curveTo(16.13f, 14.7f, 15.74f, 14.79f, 15.47f, 15.06f)
            lineTo(13.9f, 17.03f)
            curveTo(11.07f, 15.68f, 8.42f, 13.13f, 7.01f, 10.2f)
            lineTo(8.96f, 8.54f)
            curveTo(9.23f, 8.26f, 9.31f, 7.87f, 9.2f, 7.52f)
            curveTo(8.83f, 6.41f, 8.64f, 5.22f, 8.64f, 3.99f)
            curveTo(8.64f, 3.45f, 8.19f, 3f, 7.65f, 3f)
            horizontalLineTo(4.19f)
            curveTo(3.65f, 3f, 3f, 3.24f, 3f, 3.99f)
            curveTo(3f, 13.28f, 10.73f, 21f, 20.01f, 21f)
            curveTo(20.72f, 21f, 21f, 20.37f, 21f, 19.82f)
            verticalLineTo(16.37f)
            curveTo(21f, 15.83f, 20.55f, 15.38f, 20.01f, 15.38f)
            close()
        }
    }.build()
}
