package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.WarningFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "WarningFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(1f, 21.5f)
            horizontalLineTo(23f)
            lineTo(12f, 2.5f)
            lineTo(1f, 21.5f)
            close()
            moveTo(13f, 18.5f)
            horizontalLineTo(11f)
            verticalLineTo(16.5f)
            horizontalLineTo(13f)
            verticalLineTo(18.5f)
            close()
            moveTo(13f, 14.5f)
            horizontalLineTo(11f)
            verticalLineTo(10.5f)
            horizontalLineTo(13f)
            verticalLineTo(14.5f)
            close()
        }
    }.build()
}
