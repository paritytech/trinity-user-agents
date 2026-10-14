package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Pdf: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Pdf",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(20f, 2f)
            horizontalLineTo(8f)
            curveTo(6.9f, 2f, 6f, 2.9f, 6f, 4f)
            verticalLineTo(16f)
            curveTo(6f, 17.1f, 6.9f, 18f, 8f, 18f)
            horizontalLineTo(20f)
            curveTo(21.1f, 18f, 22f, 17.1f, 22f, 16f)
            verticalLineTo(4f)
            curveTo(22f, 2.9f, 21.1f, 2f, 20f, 2f)
            close()
            moveTo(20f, 16f)
            horizontalLineTo(8f)
            verticalLineTo(4f)
            horizontalLineTo(20f)
            verticalLineTo(16f)
            close()
            moveTo(4f, 6f)
            horizontalLineTo(2f)
            verticalLineTo(20f)
            curveTo(2f, 21.1f, 2.9f, 22f, 4f, 22f)
            horizontalLineTo(18f)
            verticalLineTo(20f)
            horizontalLineTo(4f)
            verticalLineTo(6f)
            close()
            moveTo(16f, 12f)
            verticalLineTo(9f)
            curveTo(16f, 8.45f, 15.55f, 8f, 15f, 8f)
            horizontalLineTo(13f)
            verticalLineTo(13f)
            horizontalLineTo(15f)
            curveTo(15.55f, 13f, 16f, 12.55f, 16f, 12f)
            close()
            moveTo(14f, 9f)
            horizontalLineTo(15f)
            verticalLineTo(12f)
            horizontalLineTo(14f)
            verticalLineTo(9f)
            close()
            moveTo(18f, 11f)
            horizontalLineTo(19f)
            verticalLineTo(10f)
            horizontalLineTo(18f)
            verticalLineTo(9f)
            horizontalLineTo(19f)
            verticalLineTo(8f)
            horizontalLineTo(17f)
            verticalLineTo(13f)
            horizontalLineTo(18f)
            verticalLineTo(11f)
            close()
            moveTo(10f, 11f)
            horizontalLineTo(11f)
            curveTo(11.55f, 11f, 12f, 10.55f, 12f, 10f)
            verticalLineTo(9f)
            curveTo(12f, 8.45f, 11.55f, 8f, 11f, 8f)
            horizontalLineTo(9f)
            verticalLineTo(13f)
            horizontalLineTo(10f)
            verticalLineTo(11f)
            close()
            moveTo(10f, 9f)
            horizontalLineTo(11f)
            verticalLineTo(10f)
            horizontalLineTo(10f)
            verticalLineTo(9f)
            close()
        }
    }.build()
}
