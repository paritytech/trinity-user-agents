package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Check: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Check",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(8.795f, 15.875f)
            lineTo(4.625f, 11.705f)
            lineTo(3.205f, 13.115f)
            lineTo(8.795f, 18.705f)
            lineTo(20.795f, 6.705f)
            lineTo(19.385f, 5.295f)
            lineTo(8.795f, 15.875f)
            close()
        }
    }.build()
}
