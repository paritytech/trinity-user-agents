package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Upload: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Upload",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(9f, 16.5f)
            horizontalLineTo(15f)
            verticalLineTo(10.5f)
            horizontalLineTo(19f)
            lineTo(12f, 3.5f)
            lineTo(5f, 10.5f)
            horizontalLineTo(9f)
            verticalLineTo(16.5f)
            close()
            moveTo(12f, 6.33f)
            lineTo(14.17f, 8.5f)
            horizontalLineTo(13f)
            verticalLineTo(14.5f)
            horizontalLineTo(11f)
            verticalLineTo(8.5f)
            horizontalLineTo(9.83f)
            lineTo(12f, 6.33f)
            close()
            moveTo(5f, 18.5f)
            horizontalLineTo(19f)
            verticalLineTo(20.5f)
            horizontalLineTo(5f)
            verticalLineTo(18.5f)
            close()
        }
    }.build()
}
