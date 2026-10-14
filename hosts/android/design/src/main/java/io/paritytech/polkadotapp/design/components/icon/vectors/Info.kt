package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Info: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Info",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(11f, 7f)
            horizontalLineTo(13f)
            verticalLineTo(9f)
            horizontalLineTo(11f)
            verticalLineTo(7f)
            close()
            moveTo(11f, 11f)
            horizontalLineTo(13f)
            verticalLineTo(17f)
            horizontalLineTo(11f)
            verticalLineTo(11f)
            close()
            moveTo(12f, 2f)
            curveTo(6.48f, 2f, 2f, 6.48f, 2f, 12f)
            curveTo(2f, 17.52f, 6.48f, 22f, 12f, 22f)
            curveTo(17.52f, 22f, 22f, 17.52f, 22f, 12f)
            curveTo(22f, 6.48f, 17.52f, 2f, 12f, 2f)
            close()
            moveTo(12f, 20f)
            curveTo(7.59f, 20f, 4f, 16.41f, 4f, 12f)
            curveTo(4f, 7.59f, 7.59f, 4f, 12f, 4f)
            curveTo(16.41f, 4f, 20f, 7.59f, 20f, 12f)
            curveTo(20f, 16.41f, 16.41f, 20f, 12f, 20f)
            close()
        }
    }.build()
}
