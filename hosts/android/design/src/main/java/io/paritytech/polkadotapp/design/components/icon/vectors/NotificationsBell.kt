package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.NotificationsBell: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "NotificationsBell",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 21.75f)
            curveTo(13.1f, 21.75f, 14f, 20.85f, 14f, 19.75f)
            horizontalLineTo(10f)
            curveTo(10f, 20.85f, 10.89f, 21.75f, 12f, 21.75f)
            close()
            moveTo(18f, 15.75f)
            verticalLineTo(10.75f)
            curveTo(18f, 7.68f, 16.36f, 5.11f, 13.5f, 4.43f)
            verticalLineTo(3.75f)
            curveTo(13.5f, 2.92f, 12.83f, 2.25f, 12f, 2.25f)
            curveTo(11.17f, 2.25f, 10.5f, 2.92f, 10.5f, 3.75f)
            verticalLineTo(4.43f)
            curveTo(7.63f, 5.11f, 6f, 7.67f, 6f, 10.75f)
            verticalLineTo(15.75f)
            lineTo(4f, 17.75f)
            verticalLineTo(18.75f)
            horizontalLineTo(20f)
            verticalLineTo(17.75f)
            lineTo(18f, 15.75f)
            close()
        }
    }.build()
}
