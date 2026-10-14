package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ArrowUp: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ArrowUp",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.White)) {
            moveTo(19.997f, 12.002f)
            lineTo(18.587f, 13.412f)
            lineTo(12.996f, 7.832f)
            verticalLineTo(20.001f)
            horizontalLineTo(10.997f)
            verticalLineTo(7.832f)
            lineTo(5.417f, 13.421f)
            lineTo(3.996f, 12.002f)
            lineTo(11.996f, 4.001f)
            lineTo(19.997f, 12.002f)
            close()
        }
    }.build()
}
