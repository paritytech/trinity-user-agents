package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.IndonesianRupiah: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "IndonesianRupiah",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 16f,
        viewportHeight = 16f
    ).apply {
        path(fill = SolidColor(Color(0xFFF0F0F0))) {
            moveTo(8f, 16f)
            curveTo(12.418f, 16f, 16f, 12.418f, 16f, 8f)
            curveTo(16f, 3.582f, 12.418f, 0f, 8f, 0f)
            curveTo(3.582f, 0f, 0f, 3.582f, 0f, 8f)
            curveTo(0f, 12.418f, 3.582f, 16f, 8f, 16f)
            close()
        }

        path(fill = SolidColor(Color(0xFFA2001D))) {
            moveTo(0f, 8f)
            curveTo(0f, 3.582f, 3.582f, 0f, 8f, 0f)
            curveTo(12.418f, 0f, 16f, 3.582f, 16f, 8f)
        }
    }.build()
}
