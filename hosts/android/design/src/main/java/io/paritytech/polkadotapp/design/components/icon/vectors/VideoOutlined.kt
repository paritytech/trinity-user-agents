package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.VideoOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "VideoOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF080808))) {
            moveTo(15f, 8f)
            curveTo(15f, 7.448f, 14.552f, 7f, 14f, 7f)
            horizontalLineTo(4f)
            curveTo(3.448f, 7f, 3f, 7.448f, 3f, 8f)
            verticalLineTo(16f)
            curveTo(3f, 16.552f, 3.448f, 17f, 4f, 17f)
            horizontalLineTo(14f)
            curveTo(14.552f, 17f, 15f, 16.552f, 15f, 16f)
            verticalLineTo(8f)
            close()
            moveTo(17.803f, 12f)
            lineTo(21f, 14.131f)
            verticalLineTo(9.868f)
            lineTo(17.803f, 12f)
            close()
            moveTo(17f, 10.131f)
            lineTo(21.445f, 7.168f)
            curveTo(21.752f, 6.963f, 22.147f, 6.944f, 22.472f, 7.118f)
            curveTo(22.797f, 7.292f, 23f, 7.631f, 23f, 8f)
            verticalLineTo(16f)
            curveTo(23f, 16.369f, 22.797f, 16.708f, 22.472f, 16.882f)
            curveTo(22.147f, 17.056f, 21.752f, 17.037f, 21.445f, 16.832f)
            lineTo(17f, 13.868f)
            verticalLineTo(16f)
            curveTo(17f, 17.657f, 15.657f, 19f, 14f, 19f)
            horizontalLineTo(4f)
            curveTo(2.343f, 19f, 1f, 17.657f, 1f, 16f)
            verticalLineTo(8f)
            curveTo(1f, 6.343f, 2.343f, 5f, 4f, 5f)
            horizontalLineTo(14f)
            curveTo(15.657f, 5f, 17f, 6.343f, 17f, 8f)
            verticalLineTo(10.131f)
            close()
        }
    }.build()
}
