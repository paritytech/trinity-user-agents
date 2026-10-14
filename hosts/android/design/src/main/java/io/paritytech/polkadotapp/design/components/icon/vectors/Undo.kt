package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Undo: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Undo",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(12.265f, 8.5f)
            curveTo(9.615f, 8.5f, 7.215f, 9.49f, 5.365f, 11.1f)
            lineTo(1.765f, 7.5f)
            verticalLineTo(16.5f)
            horizontalLineTo(10.765f)
            lineTo(7.145f, 12.88f)
            curveTo(8.535f, 11.72f, 10.305f, 11f, 12.265f, 11f)
            curveTo(15.805f, 11f, 18.815f, 13.31f, 19.865f, 16.5f)
            lineTo(22.235f, 15.72f)
            curveTo(20.845f, 11.53f, 16.915f, 8.5f, 12.265f, 8.5f)
            close()
        }
    }.build()
}
