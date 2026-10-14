package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Retry: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Retry",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(17.645f, 6.35f)
            curveTo(16.195f, 4.9f, 14.205f, 4f, 11.995f, 4f)
            curveTo(7.575f, 4f, 4.005f, 7.58f, 4.005f, 12f)
            curveTo(4.005f, 16.42f, 7.575f, 20f, 11.995f, 20f)
            curveTo(15.725f, 20f, 18.835f, 17.45f, 19.725f, 14f)
            horizontalLineTo(17.645f)
            curveTo(16.825f, 16.33f, 14.605f, 18f, 11.995f, 18f)
            curveTo(8.685f, 18f, 5.995f, 15.31f, 5.995f, 12f)
            curveTo(5.995f, 8.69f, 8.685f, 6f, 11.995f, 6f)
            curveTo(13.655f, 6f, 15.135f, 6.69f, 16.215f, 7.78f)
            lineTo(12.995f, 11f)
            horizontalLineTo(19.995f)
            verticalLineTo(4f)
            lineTo(17.645f, 6.35f)
            close()
        }
    }.build()
}
