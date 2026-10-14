package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ArrowRightShaft: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ArrowRightShaft",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(12f, 4f)
            lineTo(10.59f, 5.41f)
            lineTo(16.17f, 11f)
            horizontalLineTo(4f)
            verticalLineTo(13f)
            horizontalLineTo(16.17f)
            lineTo(10.59f, 18.59f)
            lineTo(12f, 20f)
            lineTo(20f, 12f)
            lineTo(12f, 4f)
            close()
        }
    }.build()
}
