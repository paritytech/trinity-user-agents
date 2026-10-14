package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.MessageUnreadOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "MessageUnreadOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(22f, 7.98f)
            verticalLineTo(17f)
            curveTo(22f, 18.1f, 21.1f, 19f, 20f, 19f)
            horizontalLineTo(6f)
            lineTo(2f, 23f)
            verticalLineTo(5f)
            curveTo(2f, 3.9f, 2.9f, 3f, 4f, 3f)
            horizontalLineTo(14.1f)
            curveTo(14.04f, 3.32f, 14f, 3.66f, 14f, 4f)
            curveTo(14f, 4.34f, 14.04f, 4.68f, 14.1f, 5f)
            horizontalLineTo(4f)
            verticalLineTo(17f)
            horizontalLineTo(20f)
            verticalLineTo(8.9f)
            curveTo(20.74f, 8.75f, 21.42f, 8.42f, 22f, 7.98f)
            close()
            moveTo(16f, 4f)
            curveTo(16f, 5.66f, 17.34f, 7f, 19f, 7f)
            curveTo(20.66f, 7f, 22f, 5.66f, 22f, 4f)
            curveTo(22f, 2.34f, 20.66f, 1f, 19f, 1f)
            curveTo(17.34f, 1f, 16f, 2.34f, 16f, 4f)
            close()
        }
    }.build()
}
