package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Badge: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Badge",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(18f, 12f)
            horizontalLineTo(14f)
            verticalLineTo(13.5f)
            horizontalLineTo(18f)
            verticalLineTo(12f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(18f, 15f)
            horizontalLineTo(14f)
            verticalLineTo(16.5f)
            horizontalLineTo(18f)
            verticalLineTo(15f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(20f, 7f)
            horizontalLineTo(15f)
            verticalLineTo(4f)
            curveTo(15f, 2.9f, 14.1f, 2f, 13f, 2f)
            horizontalLineTo(11f)
            curveTo(9.9f, 2f, 9f, 2.9f, 9f, 4f)
            verticalLineTo(7f)
            horizontalLineTo(4f)
            curveTo(2.9f, 7f, 2f, 7.9f, 2f, 9f)
            verticalLineTo(20f)
            curveTo(2f, 21.1f, 2.9f, 22f, 4f, 22f)
            horizontalLineTo(20f)
            curveTo(21.1f, 22f, 22f, 21.1f, 22f, 20f)
            verticalLineTo(9f)
            curveTo(22f, 7.9f, 21.1f, 7f, 20f, 7f)
            close()
            moveTo(11f, 4f)
            horizontalLineTo(13f)
            verticalLineTo(9f)
            horizontalLineTo(11f)
            verticalLineTo(4f)
            close()
            moveTo(20f, 20f)
            horizontalLineTo(4f)
            verticalLineTo(9f)
            horizontalLineTo(9f)
            curveTo(9f, 10.1f, 9.9f, 11f, 11f, 11f)
            horizontalLineTo(13f)
            curveTo(14.1f, 11f, 15f, 10.1f, 15f, 9f)
            horizontalLineTo(20f)
            verticalLineTo(20f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(9f, 15f)
            curveTo(9.828f, 15f, 10.5f, 14.328f, 10.5f, 13.5f)
            curveTo(10.5f, 12.672f, 9.828f, 12f, 9f, 12f)
            curveTo(8.172f, 12f, 7.5f, 12.672f, 7.5f, 13.5f)
            curveTo(7.5f, 14.328f, 8.172f, 15f, 9f, 15f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(11.08f, 16.18f)
            curveTo(10.44f, 15.9f, 9.74f, 15.75f, 9f, 15.75f)
            curveTo(8.26f, 15.75f, 7.56f, 15.9f, 6.92f, 16.18f)
            curveTo(6.36f, 16.42f, 6f, 16.96f, 6f, 17.57f)
            verticalLineTo(18f)
            horizontalLineTo(12f)
            verticalLineTo(17.57f)
            curveTo(12f, 16.96f, 11.64f, 16.42f, 11.08f, 16.18f)
            close()
        }
    }.build()
}
