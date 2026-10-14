package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.PriorityHigh: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "PriorityHigh",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(12f, 21f)
            curveTo(13.105f, 21f, 14f, 20.105f, 14f, 19f)
            curveTo(14f, 17.895f, 13.105f, 17f, 12f, 17f)
            curveTo(10.895f, 17f, 10f, 17.895f, 10f, 19f)
            curveTo(10f, 20.105f, 10.895f, 21f, 12f, 21f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(10f, 3f)
            horizontalLineTo(14f)
            verticalLineTo(15f)
            horizontalLineTo(10f)
            verticalLineTo(3f)
            close()
        }
    }.build()
}
