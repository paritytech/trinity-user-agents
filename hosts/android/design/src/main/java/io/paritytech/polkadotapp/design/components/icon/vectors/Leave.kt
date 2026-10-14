package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Leave: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Leave",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFFFFFFFF))) {
            moveTo(10.09f, 15.59f)
            lineTo(11.5f, 17f)
            lineTo(16.5f, 12f)
            lineTo(11.5f, 7f)
            lineTo(10.09f, 8.41f)
            lineTo(12.67f, 11f)
            horizontalLineTo(3f)
            verticalLineTo(13f)
            horizontalLineTo(12.67f)
            lineTo(10.09f, 15.59f)
            close()
            moveTo(19f, 3f)
            horizontalLineTo(5f)
            curveTo(3.89f, 3f, 3f, 3.9f, 3f, 5f)
            verticalLineTo(9f)
            horizontalLineTo(5f)
            verticalLineTo(5f)
            horizontalLineTo(19f)
            verticalLineTo(19f)
            horizontalLineTo(5f)
            verticalLineTo(15f)
            horizontalLineTo(3f)
            verticalLineTo(19f)
            curveTo(3f, 20.1f, 3.89f, 21f, 5f, 21f)
            horizontalLineTo(19f)
            curveTo(20.1f, 21f, 21f, 20.1f, 21f, 19f)
            verticalLineTo(5f)
            curveTo(21f, 3.9f, 20.1f, 3f, 19f, 3f)
            close()
        }
    }.build()
}
