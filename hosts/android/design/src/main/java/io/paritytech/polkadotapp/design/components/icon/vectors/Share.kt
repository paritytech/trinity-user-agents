package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Share: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Share",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(
            fill = SolidColor(Color.White),
        ) {
            moveTo(16f, 5f)
            lineTo(14.58f, 6.42f)
            lineTo(12.99f, 4.83f)
            verticalLineTo(16f)
            horizontalLineTo(11.01f)
            verticalLineTo(4.83f)
            lineTo(9.42f, 6.42f)
            lineTo(8f, 5f)
            lineTo(12f, 1f)
            lineTo(16f, 5f)
            close()
            moveTo(20f, 10f)
            verticalLineTo(21f)
            curveTo(20f, 22.1f, 19.1f, 23f, 18f, 23f)
            horizontalLineTo(6f)
            curveTo(4.89f, 23f, 4f, 22.1f, 4f, 21f)
            verticalLineTo(10f)
            curveTo(4f, 8.89f, 4.89f, 8f, 6f, 8f)
            horizontalLineTo(9f)
            verticalLineTo(10f)
            horizontalLineTo(6f)
            verticalLineTo(21f)
            horizontalLineTo(18f)
            verticalLineTo(10f)
            horizontalLineTo(15f)
            verticalLineTo(8f)
            horizontalLineTo(18f)
            curveTo(19.1f, 8f, 20f, 8.89f, 20f, 10f)
            close()
        }
    }.build()
}
