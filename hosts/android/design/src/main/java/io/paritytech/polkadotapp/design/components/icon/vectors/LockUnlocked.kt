package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.LockUnlocked: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "LockUnlocked",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 17.5f)
            curveTo(13.1f, 17.5f, 14f, 16.6f, 14f, 15.5f)
            curveTo(14f, 14.4f, 13.1f, 13.5f, 12f, 13.5f)
            curveTo(10.9f, 13.5f, 10f, 14.4f, 10f, 15.5f)
            curveTo(10f, 16.6f, 10.9f, 17.5f, 12f, 17.5f)
            close()
            moveTo(18f, 8.5f)
            horizontalLineTo(17f)
            verticalLineTo(6.5f)
            curveTo(17f, 3.74f, 14.76f, 1.5f, 12f, 1.5f)
            curveTo(9.24f, 1.5f, 7f, 3.74f, 7f, 6.5f)
            horizontalLineTo(8.9f)
            curveTo(8.9f, 4.79f, 10.29f, 3.4f, 12f, 3.4f)
            curveTo(13.71f, 3.4f, 15.1f, 4.79f, 15.1f, 6.5f)
            verticalLineTo(8.5f)
            horizontalLineTo(6f)
            curveTo(4.9f, 8.5f, 4f, 9.4f, 4f, 10.5f)
            verticalLineTo(20.5f)
            curveTo(4f, 21.6f, 4.9f, 22.5f, 6f, 22.5f)
            horizontalLineTo(18f)
            curveTo(19.1f, 22.5f, 20f, 21.6f, 20f, 20.5f)
            verticalLineTo(10.5f)
            curveTo(20f, 9.4f, 19.1f, 8.5f, 18f, 8.5f)
            close()
            moveTo(18f, 20.5f)
            horizontalLineTo(6f)
            verticalLineTo(10.5f)
            horizontalLineTo(18f)
            verticalLineTo(20.5f)
            close()
        }
    }.build()
}
