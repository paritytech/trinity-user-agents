package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.PermIdentityRound: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "PermIdentityRound",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 12f)
            curveTo(14.21f, 12f, 16f, 10.21f, 16f, 8f)
            curveTo(16f, 5.79f, 14.21f, 4f, 12f, 4f)
            curveTo(9.79f, 4f, 8f, 5.79f, 8f, 8f)
            curveTo(8f, 10.21f, 9.79f, 12f, 12f, 12f)
            close()
            moveTo(12f, 6f)
            curveTo(13.1f, 6f, 14f, 6.9f, 14f, 8f)
            curveTo(14f, 9.1f, 13.1f, 10f, 12f, 10f)
            curveTo(10.9f, 10f, 10f, 9.1f, 10f, 8f)
            curveTo(10f, 6.9f, 10.9f, 6f, 12f, 6f)
            close()
            moveTo(12f, 13f)
            curveTo(9.33f, 13f, 4f, 14.34f, 4f, 17f)
            verticalLineTo(19f)
            curveTo(4f, 19.55f, 4.45f, 20f, 5f, 20f)
            horizontalLineTo(19f)
            curveTo(19.55f, 20f, 20f, 19.55f, 20f, 19f)
            verticalLineTo(17f)
            curveTo(20f, 14.34f, 14.67f, 13f, 12f, 13f)
            close()
            moveTo(18f, 18f)
            horizontalLineTo(6f)
            verticalLineTo(17.01f)
            curveTo(6.2f, 16.29f, 9.3f, 15f, 12f, 15f)
            curveTo(14.7f, 15f, 17.8f, 16.29f, 18f, 17f)
            verticalLineTo(18f)
            close()
        }
    }.build()
}
