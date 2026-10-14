package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.AlertOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "AlertOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF080808))) {
            moveTo(21f, 12f)
            curveTo(21f, 7.029f, 16.971f, 3f, 12f, 3f)
            curveTo(7.029f, 3f, 3f, 7.029f, 3f, 12f)
            curveTo(3f, 16.971f, 7.029f, 21f, 12f, 21f)
            curveTo(16.971f, 21f, 21f, 16.971f, 21f, 12f)
            close()
            moveTo(12.01f, 15f)
            curveTo(12.562f, 15f, 13.01f, 15.448f, 13.01f, 16f)
            curveTo(13.01f, 16.552f, 12.562f, 17f, 12.01f, 17f)
            horizontalLineTo(12f)
            curveTo(11.448f, 17f, 11f, 16.552f, 11f, 16f)
            curveTo(11f, 15.448f, 11.448f, 15f, 12f, 15f)
            horizontalLineTo(12.01f)
            close()
            moveTo(11f, 12f)
            verticalLineTo(8f)
            curveTo(11f, 7.448f, 11.448f, 7f, 12f, 7f)
            curveTo(12.552f, 7f, 13f, 7.448f, 13f, 8f)
            verticalLineTo(12f)
            curveTo(13f, 12.552f, 12.552f, 13f, 12f, 13f)
            curveTo(11.448f, 13f, 11f, 12.552f, 11f, 12f)
            close()
            moveTo(23f, 12f)
            curveTo(23f, 18.075f, 18.075f, 23f, 12f, 23f)
            curveTo(5.925f, 23f, 1f, 18.075f, 1f, 12f)
            curveTo(1f, 5.925f, 5.925f, 1f, 12f, 1f)
            curveTo(18.075f, 1f, 23f, 5.925f, 23f, 12f)
            close()
        }
    }.build()
}
