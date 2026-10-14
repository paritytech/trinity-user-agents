package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ArrowDropdown: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ArrowDropdown",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(7.41f, 8.295f)
            lineTo(12f, 12.875f)
            lineTo(16.59f, 8.295f)
            lineTo(18f, 9.705f)
            lineTo(12f, 15.705f)
            lineTo(6f, 9.705f)
            lineTo(7.41f, 8.295f)
            close()
        }
    }.build()
}
