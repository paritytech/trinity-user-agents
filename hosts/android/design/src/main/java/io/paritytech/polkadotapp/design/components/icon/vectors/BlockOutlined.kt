package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.BlockOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "BlockOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF080808))) {
            moveTo(20f, 12f)
            curveTo(20f, 7.582f, 16.418f, 4f, 12f, 4f)
            curveTo(10.151f, 4f, 8.45f, 4.629f, 7.096f, 5.682f)
            lineTo(18.317f, 16.903f)
            curveTo(19.37f, 15.549f, 20f, 13.849f, 20f, 12f)
            close()
            moveTo(4f, 12f)
            curveTo(4f, 16.418f, 7.582f, 20f, 12f, 20f)
            curveTo(13.849f, 20f, 15.549f, 19.37f, 16.903f, 18.317f)
            lineTo(5.682f, 7.096f)
            curveTo(4.629f, 8.45f, 4f, 10.151f, 4f, 12f)
            close()
            moveTo(22f, 12f)
            curveTo(22f, 17.523f, 17.523f, 22f, 12f, 22f)
            curveTo(6.477f, 22f, 2f, 17.523f, 2f, 12f)
            curveTo(2f, 6.477f, 6.477f, 2f, 12f, 2f)
            curveTo(17.523f, 2f, 22f, 6.477f, 22f, 12f)
            close()
        }
    }.build()
}
