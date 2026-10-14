package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ArrowDownward: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "IconName",
        defaultWidth = 16.dp,
        defaultHeight = 16.dp,
        viewportWidth = 16f,
        viewportHeight = 16f
    ).apply {
        path(fill = SolidColor(Color.White)) {
            moveTo(13.331f, 7.999f)
            lineTo(12.391f, 7.059f)
            lineTo(8.664f, 10.779f)
            verticalLineTo(2.666f)
            horizontalLineTo(7.331f)
            verticalLineTo(10.779f)
            lineTo(3.611f, 7.053f)
            lineTo(2.664f, 7.999f)
            lineTo(7.997f, 13.333f)
            lineTo(13.331f, 7.999f)
            close()
        }
    }.build()
}
