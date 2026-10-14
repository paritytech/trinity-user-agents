package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.More: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "More",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(6f, 10f)
            curveTo(4.9f, 10f, 4f, 10.9f, 4f, 12f)
            curveTo(4f, 13.1f, 4.9f, 14f, 6f, 14f)
            curveTo(7.1f, 14f, 8f, 13.1f, 8f, 12f)
            curveTo(8f, 10.9f, 7.1f, 10f, 6f, 10f)
            close()
            moveTo(18f, 10f)
            curveTo(16.9f, 10f, 16f, 10.9f, 16f, 12f)
            curveTo(16f, 13.1f, 16.9f, 14f, 18f, 14f)
            curveTo(19.1f, 14f, 20f, 13.1f, 20f, 12f)
            curveTo(20f, 10.9f, 19.1f, 10f, 18f, 10f)
            close()
            moveTo(12f, 10f)
            curveTo(10.9f, 10f, 10f, 10.9f, 10f, 12f)
            curveTo(10f, 13.1f, 10.9f, 14f, 12f, 14f)
            curveTo(13.1f, 14f, 14f, 13.1f, 14f, 12f)
            curveTo(14f, 10.9f, 13.1f, 10f, 12f, 10f)
            close()
        }
    }.build()
}
