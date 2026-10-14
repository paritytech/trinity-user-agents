package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.VideoPause: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "VideoPause",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(6f, 19f)
            horizontalLineTo(10f)
            verticalLineTo(5f)
            horizontalLineTo(6f)
            verticalLineTo(19f)
            close()
            moveTo(14f, 5f)
            verticalLineTo(19f)
            horizontalLineTo(18f)
            verticalLineTo(5f)
            horizontalLineTo(14f)
            close()
        }
    }.build()
}
